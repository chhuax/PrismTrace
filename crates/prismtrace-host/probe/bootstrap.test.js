'use strict';

/**
 * Unit tests for bootstrap probe logic.
 * Uses Node.js built-in `node:test` and `node:assert` — no external dependencies.
 *
 * PRISMTRACE_PROBE_NO_AUTORUN=1 prevents the IIFE from starting the heartbeat
 * interval and stdin listener, so the test runner exits cleanly.
 *
 * Each test re-requires the module fresh via `delete require.cache[...]` to
 * reset the module-level `installedHooks` Set state.
 */

process.env.PRISMTRACE_PROBE_NO_AUTORUN = '1';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');

const BOOTSTRAP_PATH = path.resolve(__dirname, 'bootstrap.js');

function snapshotGlobalProperty(name) {
  const hasOwn = Object.prototype.hasOwnProperty.call(globalThis, name);
  return {
    hasOwn,
    value: globalThis[name],
  };
}

function restoreGlobalProperty(name, snapshot) {
  if (snapshot.hasOwn) {
    globalThis[name] = snapshot.value;
  } else {
    delete globalThis[name];
  }
}

function freshModule() {
  delete require.cache[require.resolve(BOOTSTRAP_PATH)];
  return require(BOOTSTRAP_PATH);
}

// ── 1. detectRuntimes returns http and https as available ─────────────────────

test('detectRuntimes returns http and https as available', function () {
  const { detectRuntimes, dispose } = freshModule();
  try {
    const result = detectRuntimes();
    assert.ok(Array.isArray(result.available), 'available should be an array');
    assert.ok(Array.isArray(result.unavailable), 'unavailable should be an array');
    assert.ok(result.available.includes('http'), 'http should be available');
    assert.ok(result.available.includes('https'), 'https should be available');
  } finally {
    dispose();
  }
});

// ── 2. detectRuntimes returns a valid structure ───────────────────────────────

test('detectRuntimes returns a valid structure with available and unavailable arrays', function () {
  const { detectRuntimes, dispose } = freshModule();
  try {
    const result = detectRuntimes();
    assert.ok(Array.isArray(result.available), 'available should be an array');
    assert.ok(Array.isArray(result.unavailable), 'unavailable should be an array');
    const all = result.available.concat(result.unavailable);
    assert.ok(all.includes('fetch'), 'fetch should appear in available or unavailable');
    assert.ok(all.includes('http'), 'http should appear in available or unavailable');
    assert.ok(all.includes('https'), 'https should appear in available or unavailable');
  } finally {
    dispose();
  }
});

// ── 3. installHooks installs http hook ────────────────────────────────────────

test('installHooks installs http hook', function () {
  const { installHooks, dispose } = freshModule();
  try {
    const result = installHooks(['http']);
    assert.ok(result.installedHooks.includes('http'), 'installedHooks should contain http');
    assert.deepEqual(result.failedHooks, [], 'failedHooks should be empty');
  } finally {
    dispose();
  }
});

// ── 4. installHooks is idempotent ─────────────────────────────────────────────

test('installHooks is idempotent — calling twice does not duplicate entries', function () {
  const { installHooks, dispose } = freshModule();
  try {
    installHooks(['http']);
    const result = installHooks(['http']);
    assert.equal(result.installedHooks.length, 1, 'installedHooks length should be 1 after two calls');
  } finally {
    dispose();
  }
});

// ── 5. installHooks skips unknown hook names ──────────────────────────────────

test('installHooks skips unknown hook names', function () {
  const { installHooks, dispose } = freshModule();
  try {
    const result = installHooks(['unknown_hook']);
    assert.ok(result.failedHooks.includes('unknown_hook'), 'failedHooks should contain unknown_hook');
    assert.deepEqual(result.installedHooks, [], 'installedHooks should be empty');
  } finally {
    dispose();
  }
});

// ── 6. installHooks partial failure does not abort ────────────────────────────

test('installHooks partial failure does not abort — installs valid hooks alongside failures', function () {
  const { installHooks, dispose } = freshModule();
  try {
    const result = installHooks(['http', 'unknown_hook']);
    assert.ok(result.installedHooks.includes('http'), 'installedHooks should contain http');
    assert.ok(result.failedHooks.includes('unknown_hook'), 'failedHooks should contain unknown_hook');
  } finally {
    dispose();
  }
});

// ── 7. removeAllHooks clears installed hooks ──────────────────────────────────

test('removeAllHooks clears installed hooks', function () {
  const { installHooks, removeAllHooks, dispose } = freshModule();
  try {
    installHooks(['http']);
    removeAllHooks();
    const result = installHooks(['http']);
    assert.ok(result.installedHooks.includes('http'), 'http should be installable again after removeAllHooks');
    assert.equal(result.installedHooks.length, 1, 'should have exactly one installed hook');
  } finally {
    dispose();
  }
});

test('fetch hook emits http_request_observed for JSON request bodies', async function () {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const originalFetch = globalThis.fetch;
  globalThis.fetch = async function fakeFetch() {
    return { ok: true, status: 200 };
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['fetch']);

    await globalThis.fetch('https://api.openai.com/v1/responses', {
      method: 'POST',
      headers: { authorization: 'Bearer sk-test', 'content-type': 'application/json' },
      body: '{"model":"gpt-4.1","input":"hello"}',
    });

    const observed = writes.find((chunk) => chunk.includes('"type":"http_request_observed"'));
    assert.ok(observed, 'expected one emitted request event');
    assert.match(observed, /"hook_name":"fetch"/);
    assert.ok(
      observed.includes('"url":"https://api.openai.com/v1/responses"'),
      'expected observed event to include request url'
    );
    assert.match(observed, /"method":"POST"/);
  } finally {
    process.stdout.write = originalWrite;
    globalThis.fetch = originalFetch;
    dispose();
  }
});

test('fetch hook emits matching request and response events with same exchange id', async function () {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const originalFetch = globalThis.fetch;
  globalThis.fetch = async function fakeFetch() {
    return new Response('{"ok":true}', {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['fetch']);

    await globalThis.fetch('https://api.openai.com/v1/responses', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: '{"model":"gpt-test","input":"hi"}',
    });

    await new Promise((resolve) => setTimeout(resolve, 0));

    const requestEvent = writes.find((line) => line.includes('"type":"http_request_observed"'));
    const responseEvent = writes.find((line) => line.includes('"type":"http_response_observed"'));

    assert.ok(requestEvent, 'expected request event');
    assert.ok(responseEvent, 'expected response event');

    const requestJson = JSON.parse(requestEvent);
    const responseJson = JSON.parse(responseEvent);
    assert.equal(requestJson.exchange_id, responseJson.exchange_id);
    assert.equal(responseJson.status_code, 200);
  } finally {
    process.stdout.write = originalWrite;
    globalThis.fetch = originalFetch;
    dispose();
  }
});

test('fetch hook preserves an explicit zero timestamp for observed requests', async function () {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const originalNow = Date.now;
  Date.now = function () {
    return 0;
  };

  const originalFetch = globalThis.fetch;
  globalThis.fetch = async function fakeFetch() {
    return new Response('{"ok":true}', {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['fetch']);

    await globalThis.fetch('https://api.openai.com/v1/responses', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: '{"model":"gpt-test","input":"hi"}',
    });

    const requestEvent = writes.find((line) => line.includes('"type":"http_request_observed"'));
    assert.ok(requestEvent, 'expected request event');

    const requestJson = JSON.parse(requestEvent);
    assert.equal(requestJson.timestamp_ms, 0);
  } finally {
    process.stdout.write = originalWrite;
    Date.now = originalNow;
    globalThis.fetch = originalFetch;
    dispose();
  }
});

test('http hook ignores non-text request bodies without throwing', function () {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const http = require('http');
  const originalRequest = http.request;
  http.request = function fakeRequest() {
    return {
      on() {},
      once() {},
      write() {},
      end() {},
    };
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['http']);
    http.request('https://api.anthropic.com/v1/messages', {
      method: 'POST',
      headers: { 'x-api-key': 'test' },
    });

    const observed = writes.find((chunk) => chunk.includes('"type":"http_request_observed"'));
    assert.ok(observed, 'expected one emitted request event');
    assert.match(observed, /"hook_name":"http"/);
  } finally {
    process.stdout.write = originalWrite;
    http.request = originalRequest;
    dispose();
  }
});

test('fetch hook swallows observation errors and still calls original fetch', async function () {
  let called = false;
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async function fakeFetch() {
    called = true;
    return { ok: true, status: 200 };
  };

  const { installHooks, dispose } = freshModule();

  try {
    installHooks(['fetch']);

    await assert.doesNotReject(async function () {
      await globalThis.fetch('https://api.openai.com/v1/responses', {
        method: 'POST',
        body: {
          toString() {
            throw new Error('boom');
          },
        },
      });
    });

    assert.equal(called, true, 'original fetch should still be called');
  } finally {
    process.stdout.write = originalWrite;
    globalThis.fetch = originalFetch;
    dispose();
  }
});

test('sendMessage prefers global bridge emitter over stdout when bridge exists', function () {
  const lines = [];
  const writes = [];
  const bridgeSnapshot = snapshotGlobalProperty('__prismtraceEmit');
  const originalWrite = process.stdout.write;

  globalThis.__prismtraceEmit = function (line) {
    lines.push(String(line));
  };
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const { sendMessage, dispose } = freshModule();

  try {
    sendMessage({ type: 'heartbeat', timestamp_ms: 1 });
    assert.equal(lines.length, 1, 'bridge should receive one line');
    assert.equal(writes.length, 0, 'stdout should not be used when bridge exists');
    assert.match(lines[0], /"type":"heartbeat"/);
    assert.ok(lines[0].endsWith('\n'), 'bridge line should end with newline');
  } finally {
    process.stdout.write = originalWrite;
    restoreGlobalProperty('__prismtraceEmit', bridgeSnapshot);
    dispose();
  }
});

test('sendMessage falls back to stdout when bridge emitter is absent', function () {
  const writes = [];
  const bridgeSnapshot = snapshotGlobalProperty('__prismtraceEmit');
  const originalWrite = process.stdout.write;

  delete globalThis.__prismtraceEmit;
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const { sendMessage, dispose } = freshModule();

  try {
    sendMessage({ type: 'heartbeat', timestamp_ms: 2 });
    assert.equal(writes.length, 1, 'stdout should receive one line without bridge');
    assert.match(writes[0], /"type":"heartbeat"/);
    assert.ok(writes[0].endsWith('\n'), 'stdout line should end with newline');
  } finally {
    process.stdout.write = originalWrite;
    restoreGlobalProperty('__prismtraceEmit', bridgeSnapshot);
    dispose();
  }
});

test('triggerDetach emits detach_ack and disposes installed hooks', function () {
  const lines = [];
  const bridgeSnapshot = snapshotGlobalProperty('__prismtraceEmit');
  const originalFetch = globalThis.fetch;

  globalThis.__prismtraceEmit = function (line) {
    lines.push(String(line));
  };
  globalThis.fetch = function fakeFetch() {
    return Promise.resolve({ ok: true, status: 200 });
  };

  const fakeFetchRef = globalThis.fetch;
  const { installHooks, triggerDetach, dispose } = freshModule();

  try {
    installHooks(['fetch']);
    assert.notEqual(globalThis.fetch, fakeFetchRef, 'fetch should be patched after hook install');

    triggerDetach();
    assert.equal(globalThis.fetch, fakeFetchRef, 'fetch hook should be restored on detach');

    const ack = lines.find((line) => line.includes('"type":"detach_ack"'));
    assert.ok(ack, 'detach_ack should be emitted');
    assert.ok(ack.endsWith('\n'), 'detach ack should end with newline');
  } finally {
    restoreGlobalProperty('__prismtraceEmit', bridgeSnapshot);
    globalThis.fetch = originalFetch;
    dispose();
  }
});

test('sendMessage swallows bridge emitter errors', function () {
  const bridgeSnapshot = snapshotGlobalProperty('__prismtraceEmit');
  const writes = [];
  const originalWrite = process.stdout.write;

  globalThis.__prismtraceEmit = function () {
    throw new Error('bridge boom');
  };
  process.stdout.write = function (chunk) {
    writes.push(String(chunk));
    return true;
  };

  const { sendMessage, dispose } = freshModule();

  try {
    assert.doesNotThrow(function () {
      sendMessage({ type: 'heartbeat', timestamp_ms: 3 });
    });
    assert.equal(writes.length, 0, 'stdout should not be used when bridge emitter throws');
  } finally {
    process.stdout.write = originalWrite;
    restoreGlobalProperty('__prismtraceEmit', bridgeSnapshot);
    dispose();
  }
});

test('sendMessage swallows stdout errors when bridge is absent', function () {
  const bridgeSnapshot = snapshotGlobalProperty('__prismtraceEmit');
  const originalWrite = process.stdout.write;

  delete globalThis.__prismtraceEmit;
  process.stdout.write = function () {
    throw new Error('stdout boom');
  };

  const { sendMessage, dispose } = freshModule();

  try {
    assert.doesNotThrow(function () {
      sendMessage({ type: 'heartbeat', timestamp_ms: 4 });
    });
  } finally {
    process.stdout.write = originalWrite;
    restoreGlobalProperty('__prismtraceEmit', bridgeSnapshot);
    dispose();
  }
});

test('triggerDetach always disposes hooks when detach ack emit fails', function () {
  const bridgeSnapshot = snapshotGlobalProperty('__prismtraceEmit');
  const originalFetch = globalThis.fetch;

  globalThis.__prismtraceEmit = function () {
    throw new Error('detach emit failed');
  };
  globalThis.fetch = function fakeFetch() {
    return Promise.resolve({ ok: true, status: 200 });
  };

  const fakeFetchRef = globalThis.fetch;
  const { installHooks, triggerDetach, dispose } = freshModule();

  try {
    installHooks(['fetch']);
    assert.notEqual(globalThis.fetch, fakeFetchRef, 'fetch should be patched after hook install');

    assert.doesNotThrow(function () {
      triggerDetach();
    });
    assert.equal(globalThis.fetch, fakeFetchRef, 'fetch should be restored even when ack emit fails');
  } finally {
    restoreGlobalProperty('__prismtraceEmit', bridgeSnapshot);
    globalThis.fetch = originalFetch;
    dispose();
  }
});

test('dispose clears registered global detach helper', function () {
  const detachSnapshot = snapshotGlobalProperty('__prismtraceDetach');
  const { triggerDetach, dispose } = freshModule();

  globalThis.__prismtraceDetach = triggerDetach;

  try {
    dispose();
    assert.equal(
      Object.prototype.hasOwnProperty.call(globalThis, '__prismtraceDetach'),
      false,
      '__prismtraceDetach should be removed by dispose'
    );
  } finally {
    restoreGlobalProperty('__prismtraceDetach', detachSnapshot);
  }
});
