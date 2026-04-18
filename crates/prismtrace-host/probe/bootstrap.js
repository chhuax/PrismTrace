/**
 * PrismTrace Bootstrap Probe
 *
 * Self-executing IIFE injected into a target Node / Electron process.
 * Responsibilities:
 *   1. Detect available hook points (fetch, undici, http, https)
 *   2. Install no-op hook skeletons for each available hook point
 *   3. Send BootstrapReport via newline-delimited JSON on process.stdout
 *   4. Maintain a heartbeat and handle detach
 */
(function bootstrapProbe() {
  'use strict';

  // ── Constants ────────────────────────────────────────────────────────────────

  var HEARTBEAT_INTERVAL_MS = 5000;

  // ── IPC helpers ──────────────────────────────────────────────────────────────

  /**
   * Send a message to the host.
   *
   * Transport: newline-delimited JSON written to process.stdout.
   * This is the canonical transport consumed by the host's IpcListener (read_line).
   *
   * process.send() is NOT used — it serialises JS objects through Node's IPC
   * channel protocol, which is incompatible with the host's line-oriented JSON reader.
   */
  function sendMessage(msg) {
    if (typeof process !== 'undefined' && process.stdout && typeof process.stdout.write === 'function') {
      process.stdout.write(JSON.stringify(msg) + '\n');
    }
  }

  // ── Runtime detection ────────────────────────────────────────────────────────

  /**
   * Detect which hook points are available in the current process.
   * Returns { available: string[], unavailable: string[] }
   */
  function detectRuntimes() {
    var available = [];
    var unavailable = [];

    // fetch — global function
    if (typeof globalThis.fetch === 'function') {
      available.push('fetch');
    } else {
      unavailable.push('fetch');
    }

    // undici — CommonJS module
    try {
      require('undici');
      available.push('undici');
    } catch (_) {
      unavailable.push('undici');
    }

    // http — built-in Node module
    try {
      require('http');
      available.push('http');
    } catch (_) {
      unavailable.push('http');
    }

    // https — built-in Node module
    try {
      require('https');
      available.push('https');
    } catch (_) {
      unavailable.push('https');
    }

    return { available: available, unavailable: unavailable };
  }

  // ── Hook skeleton installation ───────────────────────────────────────────────

  // Track installed hooks for idempotency and later removal.
  var installedHooks = new Set();

  // Store original references so we can restore them on detach.
  var originals = {};

  /**
   * Install no-op wrapper hooks for each available hook point.
   * Returns { installedHooks: string[], failedHooks: string[] }
   */
  function installHooks(available) {
    var installed = [];
    var failed = [];

    available.forEach(function (hookName) {
      // Idempotency: skip if already installed.
      if (installedHooks.has(hookName)) {
        installed.push(hookName);
        return;
      }

      try {
        switch (hookName) {
          case 'fetch': {
            var originalFetch = globalThis.fetch;
            originals['fetch'] = originalFetch;
            globalThis.fetch = function patchedFetch() {
              return originalFetch.apply(this, arguments);
            };
            installedHooks.add('fetch');
            installed.push('fetch');
            break;
          }

          case 'undici': {
            var undici = require('undici');
            var originalRequest = undici.request;
            originals['undici'] = originalRequest;
            undici.request = function patchedUndiciRequest() {
              return originalRequest.apply(this, arguments);
            };
            installedHooks.add('undici');
            installed.push('undici');
            break;
          }

          case 'http': {
            var http = require('http');
            var originalHttpRequest = http.request;
            originals['http'] = originalHttpRequest;
            http.request = function patchedHttpRequest() {
              return originalHttpRequest.apply(this, arguments);
            };
            installedHooks.add('http');
            installed.push('http');
            break;
          }

          case 'https': {
            var https = require('https');
            var originalHttpsRequest = https.request;
            originals['https'] = originalHttpsRequest;
            https.request = function patchedHttpsRequest() {
              return originalHttpsRequest.apply(this, arguments);
            };
            installedHooks.add('https');
            installed.push('https');
            break;
          }

          default:
            failed.push(hookName);
            break;
        }
      } catch (_err) {
        failed.push(hookName);
      }
    });

    return { installedHooks: installed, failedHooks: failed };
  }

  // ── Detach / cleanup ─────────────────────────────────────────────────────────

  function removeAllHooks() {
    installedHooks.forEach(function (hookName) {
      try {
        switch (hookName) {
          case 'fetch':
            if (originals['fetch'] !== undefined) {
              globalThis.fetch = originals['fetch'];
            }
            break;
          case 'undici': {
            var undici = require('undici');
            if (originals['undici'] !== undefined) {
              undici.request = originals['undici'];
            }
            break;
          }
          case 'http': {
            var http = require('http');
            if (originals['http'] !== undefined) {
              http.request = originals['http'];
            }
            break;
          }
          case 'https': {
            var https = require('https');
            if (originals['https'] !== undefined) {
              https.request = originals['https'];
            }
            break;
          }
        }
      } catch (_) {
        // Best-effort cleanup; ignore errors during detach.
      }
    });

    installedHooks.clear();
    originals = {};
  }

  // ── Bootstrap sequence ───────────────────────────────────────────────────────
  //
  // Side-effectful startup (heartbeat, stdin listener, BootstrapReport) is gated
  // behind PRISMTRACE_PROBE_NO_AUTORUN so unit tests can require() this file to
  // access the exported helpers without starting timers that keep the process alive.

  var heartbeatInterval = null;

  function dispose() {
    if (heartbeatInterval !== null) {
      clearInterval(heartbeatInterval);
      heartbeatInterval = null;
    }
    if (typeof process !== 'undefined' && process.stdin) {
      process.stdin.pause();
    }
    removeAllHooks();
  }

  var isTestMode =
    typeof process !== 'undefined' &&
    process.env &&
    process.env.PRISMTRACE_PROBE_NO_AUTORUN === '1';

  if (!isTestMode) {
    var detection = detectRuntimes();
    var hookResult = installHooks(detection.available);

    // Send BootstrapReport.
    sendMessage({
      type: 'bootstrap_report',
      installed_hooks: hookResult.installedHooks,
      failed_hooks: hookResult.failedHooks,
      timestamp_ms: Date.now(),
    });

    // ── Heartbeat ──────────────────────────────────────────────────────────────

    heartbeatInterval = setInterval(function () {
      sendMessage({
        type: 'heartbeat',
        timestamp_ms: Date.now(),
      });
    }, HEARTBEAT_INTERVAL_MS);

    // ── Detach listener ────────────────────────────────────────────────────────
    //
    // Detach commands arrive as newline-delimited JSON on process.stdin,
    // matching the same transport used for outbound IPC messages.

    if (typeof process !== 'undefined' && process.stdin && typeof process.stdin.on === 'function') {
      var stdinBuf = '';
      process.stdin.setEncoding('utf8');
      process.stdin.on('data', function (chunk) {
        stdinBuf += chunk;
        var lines = stdinBuf.split('\n');
        stdinBuf = lines.pop(); // keep incomplete last line
        lines.forEach(function (line) {
          if (!line.trim()) return;
          try {
            var msg = JSON.parse(line);
            if (msg && msg.type === 'detach') {
              sendMessage({ type: 'detach_ack', timestamp_ms: Date.now() });
              dispose();
            }
          } catch (_) {
            // Ignore malformed lines.
          }
        });
      });
    }
  }

  // Export internals for testing (only when running under Node.js module system)
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = { detectRuntimes, installHooks, removeAllHooks, dispose };
  }
})();
