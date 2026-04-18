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
  var BODY_TEXT_LIMIT_BYTES = 64 * 1024;

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

  function normalizeHeaders(headers) {
    if (!headers) {
      return [];
    }

    if (Array.isArray(headers)) {
      return headers.map(function (entry) {
        return {
          name: String(entry[0]).toLowerCase(),
          value: String(entry[1]),
        };
      });
    }

    if (typeof headers.forEach === 'function') {
      var normalized = [];
      headers.forEach(function (value, name) {
        normalized.push({
          name: String(name).toLowerCase(),
          value: String(value),
        });
      });
      return normalized;
    }

    return Object.keys(headers).map(function (name) {
      return {
        name: String(name).toLowerCase(),
        value: String(headers[name]),
      };
    });
  }

  function toBodyText(body) {
    function truncatedText(text) {
      text = String(text);
      return {
        text: text.slice(0, BODY_TEXT_LIMIT_BYTES),
        truncated: text.length > BODY_TEXT_LIMIT_BYTES,
      };
    }

    if (typeof body === 'string') {
      return truncatedText(body);
    }

    if (body && typeof body === 'object' && typeof body.toString === 'function') {
      var text = body.toString();
      if (text !== '[object Object]') {
        return truncatedText(text);
      }
    }

    return { text: null, truncated: false };
  }

  function emitObservedRequest(observed) {
    sendMessage({
      type: 'http_request_observed',
      hook_name: observed.hookName,
      method: observed.method,
      url: observed.url,
      headers: observed.headers,
      body_text: observed.bodyText,
      body_truncated: observed.bodyTruncated,
      timestamp_ms: Date.now(),
    });
  }

  function observeRequestSafely(factory) {
    try {
      emitObservedRequest(factory());
    } catch (_) {
      // Observation must never change request semantics.
    }
  }

  function toUrlString(input) {
    if (typeof input === 'string') {
      return input;
    }

    if (input && typeof input.url === 'string') {
      return input.url;
    }

    return String(input);
  }

  function requestUrlFromHttpArgs(firstArg, secondArg, moduleName) {
    if (typeof firstArg === 'string') {
      return firstArg;
    }

    var options = firstArg || secondArg || {};
    if (options && typeof options.href === 'string') {
      return options.href;
    }

    var protocol = options.protocol || moduleName + ':';
    var hostname = options.hostname || options.host || 'localhost';
    var port = options.port ? ':' + options.port : '';
    var path = options.path || '/';
    return protocol + '//' + hostname + port + path;
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
            globalThis.fetch = function patchedFetch(input, init) {
              observeRequestSafely(function () {
                var method = (init && init.method) || (input && input.method) || 'GET';
                var headers = normalizeHeaders((init && init.headers) || (input && input.headers) || {});
                var bodySource = init && Object.prototype.hasOwnProperty.call(init, 'body') ? init.body : input && input.body;
                var bodyInfo = toBodyText(bodySource);
                return {
                  hookName: 'fetch',
                  method: String(method).toUpperCase(),
                  url: toUrlString(input),
                  headers: headers,
                  bodyText: bodyInfo.text,
                  bodyTruncated: bodyInfo.truncated,
                };
              });
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
            undici.request = function patchedUndiciRequest(url, options) {
              options = options || {};
              observeRequestSafely(function () {
                var bodyInfo = toBodyText(options.body);
                return {
                  hookName: 'undici',
                  method: String(options.method || 'GET').toUpperCase(),
                  url: toUrlString(url),
                  headers: normalizeHeaders(options.headers || {}),
                  bodyText: bodyInfo.text,
                  bodyTruncated: bodyInfo.truncated,
                };
              });
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
            http.request = function patchedHttpRequest(options, callbackOptions) {
              var requestOptions =
                typeof options === 'string' || options instanceof URL ? callbackOptions || {} : options || {};
              observeRequestSafely(function () {
                var bodyInfo = toBodyText(requestOptions.body);
                return {
                  hookName: 'http',
                  method: String(requestOptions.method || 'GET').toUpperCase(),
                  url: requestUrlFromHttpArgs(options, callbackOptions, 'http'),
                  headers: normalizeHeaders(requestOptions.headers || {}),
                  bodyText: bodyInfo.text,
                  bodyTruncated: bodyInfo.truncated,
                };
              });
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
            https.request = function patchedHttpsRequest(options, callbackOptions) {
              var requestOptions =
                typeof options === 'string' || options instanceof URL ? callbackOptions || {} : options || {};
              observeRequestSafely(function () {
                var bodyInfo = toBodyText(requestOptions.body);
                return {
                  hookName: 'https',
                  method: String(requestOptions.method || 'GET').toUpperCase(),
                  url: requestUrlFromHttpArgs(options, callbackOptions, 'https'),
                  headers: normalizeHeaders(requestOptions.headers || {}),
                  bodyText: bodyInfo.text,
                  bodyTruncated: bodyInfo.truncated,
                };
              });
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
