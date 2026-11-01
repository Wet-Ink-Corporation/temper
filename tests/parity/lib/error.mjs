// The harness's own failure: a spawn that failed, timed out or overflowed, output that is not the
// JSON it should be, a missing binary or an empty corpus root. It is never a parity result, so the
// corpus driver exits 2 on it and a test file fails on it; nothing is reported as skipped (H-12).
export class HarnessError extends Error {
  constructor(message, options) {
    super(message, options);
    this.name = 'HarnessError';
  }
}
