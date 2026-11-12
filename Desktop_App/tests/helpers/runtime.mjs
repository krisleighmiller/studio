import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';

export const appRoot = path.resolve(fileURLToPath(new URL('../../', import.meta.url)));

export function prepareOutput(defaultLabel, requested = process.env.STUDIO_TEST_OUT) {
  if (requested === undefined) {
    return fs.mkdtempSync(path.join(os.tmpdir(), `${defaultLabel}-`));
  }
  const out = path.resolve(requested);
  fs.mkdirSync(out);
  return out;
}
