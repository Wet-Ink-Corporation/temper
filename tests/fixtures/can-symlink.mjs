// Whether this host can create symlinks. Windows refuses them without Developer
// Mode or an elevated shell; the tests guard their symlink cases with this.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export const canSymlink = (() => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'temper-symlink-'));
  try {
    fs.writeFileSync(path.join(dir, 'target'), '');
    fs.symlinkSync(path.join(dir, 'target'), path.join(dir, 'link'));
    return true;
  } catch {
    return false;
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
})();
