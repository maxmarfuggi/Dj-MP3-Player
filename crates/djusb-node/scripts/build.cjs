const { spawnSync } = require('node:child_process');
const { existsSync } = require('node:fs');
const { homedir } = require('node:os');
const path = require('node:path');

// rustup installs cargo here. A terminal opened before that install, or one
// that did not pick up the user PATH, can still build.
const env = { ...process.env };
// Windows keeps this as `Path`. Assigning `PATH` as well would replace it with
// only the cargo directory, and Node would then be missing.
const pathKey = Object.keys(env).find((key) => key.toLowerCase() === 'path') ?? 'PATH';
const cargoDir = path.join(homedir(), '.cargo', 'bin');
const cargoExe = path.join(cargoDir, process.platform === 'win32' ? 'cargo.exe' : 'cargo');
const entries = (env[pathKey] ?? '').split(path.delimiter).filter(Boolean);
const listed = entries.some((entry) => path.resolve(entry) === path.resolve(cargoDir));
if (existsSync(cargoExe) && !listed) {
  env[pathKey] = `${cargoDir}${path.delimiter}${env[pathKey] ?? ''}`;
}

// The vendored OpenSSL build runs Perl's Configure. Git for Windows ships a
// Perl that cannot do that, so Strawberry Perl has to come first when it is
// installed.
const perlDirs = [
  'C:\\Strawberry\\perl\\bin',
  path.join(process.env.ProgramFiles ?? 'C:\\Program Files', 'Strawberry', 'perl', 'bin'),
];
const perlExe = process.platform === 'win32' ? 'perl.exe' : 'perl';
if (process.platform === 'win32') {
  const perlDir = perlDirs.find((dir) => existsSync(path.join(dir, perlExe)));
  if (perlDir !== undefined) {
    const current = (env[pathKey] ?? '').split(path.delimiter).filter(Boolean);
    const rest = current.filter((entry) => path.resolve(entry) !== path.resolve(perlDir));
    env[pathKey] = [perlDir, ...rest].join(path.delimiter);
  }
}

const cargo = existsSync(cargoExe) ? cargoExe : 'cargo';
const probe = spawnSync(cargo, ['--version'], { env, encoding: 'utf8' });
if (probe.status !== 0) {
  console.error('cargo was not found. Install Rust from https://rustup.rs, then run this again.');
  process.exit(1);
}

// Git's Perl answers `perl -v` but has no Win32 module, which Configure needs.
const perlArgs = process.platform === 'win32' ? ['-MWin32', '-e', '1'] : ['-v'];
const perl = spawnSync('perl', perlArgs, { env, encoding: 'utf8' });
if (perl.status !== 0) {
  console.error('perl was not found. Building OpenSSL needs Strawberry Perl: https://strawberryperl.com');
  process.exit(1);
}

const napiName = process.platform === 'win32' ? 'napi.cmd' : 'napi';
const napi = [
  path.join(__dirname, '..', 'node_modules', '.bin', napiName),
  path.join(__dirname, '..', '..', '..', 'node_modules', '.bin', napiName),
].find((candidate) => existsSync(candidate));
if (napi === undefined) {
  console.error('The napi command was not found. Run npm install from the repository root.');
  process.exit(1);
}

const result = spawnSync(
  napi,
  ['build', '--platform', '--manifest-path', './Cargo.toml', ...process.argv.slice(2)],
  {
    stdio: 'inherit',
    env,
    shell: process.platform === 'win32',
  },
);
process.exit(result.status ?? 1);
