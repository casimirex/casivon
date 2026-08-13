import { describe, expect, it } from 'vitest';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

/**
 * The wire types are generated in two hops:
 *
 *   Rust DTOs --(cargo run --bin openapi)--> openapi.json --(openapi-typescript)--> schema.d.ts
 *
 * Both files are committed so a clone builds without a Rust toolchain, which
 * means both can go stale. This test guards the second hop; a test on the
 * backend side (`tests/openapi.rs`) guards the first, where cargo is available.
 */
describe('generated schema', () => {
  it('is up to date with openapi.json', () => {
    const temp = mkdtempSync(join(tmpdir(), 'erp-schema-'));
    const regenerated = join(temp, 'schema.d.ts');

    try {
      execFileSync(
        'npx',
        ['openapi-typescript', 'openapi.json', '-o', regenerated],
        { stdio: 'pipe' }
      );

      expect(
        readFileSync('src/api/schema.d.ts', 'utf8'),
        'src/api/schema.d.ts is stale — run `npm run generate:types`'
      ).toBe(readFileSync(regenerated, 'utf8'));
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  }, 60_000);
});
