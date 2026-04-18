'use strict';

/**
 * Unit tests for bootstrap probe logic.
 * Uses Node.js built-in `node:test` and `node:assert` — no external dependencies.
 *
 * Each test re-requires the module fresh via `delete require.cache[...]` to
 * reset the module-level `installedHooks` Set state.
 */

const { test, beforeEach } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');

const BOOTSTRAP_PATH = path.resolve(__dirname, 'bootstrap.js');

function freshModule() {
  delete require.cache[require.resolve(BOOTSTRAP_PATH)];
  return require(BOOTSTRAP_PATH);
}

// ── 1. detectRuntimes returns http and https as available ─────────────────────

test('detectRuntimes returns http and https as available', function () {
  const { detectRuntimes } = freshModule();
  const result = detectRuntimes();

  assert.ok(Array.isArray(result.available), 'available should be an array');
  assert.ok(Array.isArray(result.unavailable), 'unavailable should be an array');
  assert.ok(result.available.includes('http'), 'http should be available');
  assert.ok(result.available.includes('https'), 'https should be available');
});

// ── 2. detectRuntimes returns a valid structure ───────────────────────────────

test('detectRuntimes returns a valid structure with available and unavailable arrays', function () {
  const { detectRuntimes } = freshModule();
  const result = detectRuntimes();

  assert.ok(Array.isArray(result.available), 'available should be an array');
  assert.ok(Array.isArray(result.unavailable), 'unavailable should be an array');

  // Every hook name should appear in exactly one of the two arrays.
  const all = result.available.concat(result.unavailable);
  assert.ok(all.includes('fetch'), 'fetch should appear in available or unavailable');
  assert.ok(all.includes('http'), 'http should appear in available or unavailable');
  assert.ok(all.includes('https'), 'https should appear in available or unavailable');
});

// ── 3. installHooks installs http hook ────────────────────────────────────────

test('installHooks installs http hook', function () {
  const { installHooks, removeAllHooks } = freshModule();
  try {
    const result = installHooks(['http']);
    assert.ok(result.installedHooks.includes('http'), 'installedHooks should contain http');
    assert.deepEqual(result.failedHooks, [], 'failedHooks should be empty');
  } finally {
    removeAllHooks();
  }
});

// ── 4. installHooks is idempotent ─────────────────────────────────────────────

test('installHooks is idempotent — calling twice does not duplicate entries', function () {
  const { installHooks, removeAllHooks } = freshModule();
  try {
    installHooks(['http']);
    const result = installHooks(['http']);
    assert.equal(result.installedHooks.length, 1, 'installedHooks length should be 1 after two calls');
  } finally {
    removeAllHooks();
  }
});

// ── 5. installHooks skips unknown hook names ──────────────────────────────────

test('installHooks skips unknown hook names', function () {
  const { installHooks, removeAllHooks } = freshModule();
  try {
    const result = installHooks(['unknown_hook']);
    assert.ok(result.failedHooks.includes('unknown_hook'), 'failedHooks should contain unknown_hook');
    assert.deepEqual(result.installedHooks, [], 'installedHooks should be empty');
  } finally {
    removeAllHooks();
  }
});

// ── 6. installHooks partial failure does not abort ────────────────────────────

test('installHooks partial failure does not abort — installs valid hooks alongside failures', function () {
  const { installHooks, removeAllHooks } = freshModule();
  try {
    const result = installHooks(['http', 'unknown_hook']);
    assert.ok(result.installedHooks.includes('http'), 'installedHooks should contain http');
    assert.ok(result.failedHooks.includes('unknown_hook'), 'failedHooks should contain unknown_hook');
  } finally {
    removeAllHooks();
  }
});

// ── 7. removeAllHooks clears installed hooks ──────────────────────────────────

test('removeAllHooks clears installed hooks', function () {
  const { installHooks, removeAllHooks } = freshModule();
  installHooks(['http']);
  removeAllHooks();

  // After removal, installing again should succeed (not be treated as already-installed).
  const result = installHooks(['http']);
  assert.ok(result.installedHooks.includes('http'), 'http should be installable again after removeAllHooks');
  assert.equal(result.installedHooks.length, 1, 'should have exactly one installed hook');
  removeAllHooks();
});
