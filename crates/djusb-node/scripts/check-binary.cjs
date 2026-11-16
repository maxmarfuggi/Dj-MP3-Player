const fs = require('node:fs');
const path = require('node:path');

// Confirms a prebuilt `.node` is the architecture its file name claims, and,
// when asked, that a Linux build does not require a newer glibc than the
// cross toolchain's floor.

const EXPECTED = {
  'win32-x64-msvc': { format: 'pe', machine: 0x8664 },
  'win32-arm64-msvc': { format: 'pe', machine: 0xaa64 },
  'darwin-x64': { format: 'macho', cpu: 0x01000007 },
  'darwin-arm64': { format: 'macho', cpu: 0x0100000c },
  'linux-x64-gnu': { format: 'elf', machine: 0x3e },
  'linux-arm64-gnu': { format: 'elf', machine: 0xb7 },
};

const ELF_VERNEED = 0x6ffffffe;
const ELF_STRTAB = 3;

function idFromName(file) {
  const base = path.basename(file);
  const prefix = 'djusb-node.';
  const suffix = '.node';
  if (!base.startsWith(prefix) || !base.endsWith(suffix)) return undefined;
  return base.slice(prefix.length, -suffix.length);
}

function readElf(buf) {
  if (buf.length < 64 || buf[0] !== 0x7f || buf[1] !== 0x45 || buf[2] !== 0x4c || buf[3] !== 0x46) {
    return undefined;
  }
  if (buf[4] !== 2 || buf[5] !== 1) {
    throw new Error('expected a 64-bit little-endian ELF file');
  }
  return { format: 'elf', machine: buf.readUInt16LE(18) };
}

function readMachO(buf) {
  if (buf.length < 8 || buf.readUInt32LE(0) !== 0xfeedfacf) return undefined;
  return { format: 'macho', cpu: buf.readUInt32LE(4) };
}

function readPe(buf) {
  if (buf.length < 0x40 || buf[0] !== 0x4d || buf[1] !== 0x5a) return undefined;
  const pe = buf.readUInt32LE(0x3c);
  if (pe <= 0 || pe + 6 > buf.length || buf.readUInt32LE(pe) !== 0x4550) {
    throw new Error('expected a PE file');
  }
  return { format: 'pe', machine: buf.readUInt16LE(pe + 4) };
}

function identify(buf) {
  return readElf(buf) ?? readMachO(buf) ?? readPe(buf);
}

function section(buf, index) {
  const count = buf.readUInt16LE(60);
  const headers = Number(buf.readBigUInt64LE(40));
  if (index >= count) throw new Error('ELF section index is past the section table');
  const at = headers + index * 64;
  if (at + 64 > buf.length) throw new Error('ELF section header is outside the file');
  return {
    name: buf.readUInt32LE(at),
    type: buf.readUInt32LE(at + 4),
    offset: Number(buf.readBigUInt64LE(at + 24)),
    size: Number(buf.readBigUInt64LE(at + 32)),
    link: buf.readUInt32LE(at + 40),
  };
}

function slice(buf, offset, size, label) {
  if (offset < 0 || size < 0 || offset + size > buf.length) {
    throw new Error(`${label} is outside the file`);
  }
  return buf.subarray(offset, offset + size);
}

function cString(buf, offset) {
  const end = buf.indexOf(0, offset);
  if (end < 0) throw new Error('ELF string is not terminated');
  return buf.toString('latin1', offset, end);
}

function glibcVersions(buf) {
  const count = buf.readUInt16LE(60);
  const found = [];
  for (let index = 0; index < count; index += 1) {
    const header = section(buf, index);
    if (header.type !== ELF_VERNEED) continue;
    const strings = section(buf, header.link);
    if (strings.type !== ELF_STRTAB) throw new Error('ELF version section does not link a string table');
    const table = slice(buf, strings.offset, strings.size, 'ELF string table');
    const body = slice(buf, header.offset, header.size, 'ELF version section');
    let needAt = 0;
    for (;;) {
      if (needAt + 16 > body.length) throw new Error('ELF version record is outside its section');
      const auxCount = body.readUInt16LE(needAt + 2);
      let auxAt = needAt + body.readUInt32LE(needAt + 8);
      for (let aux = 0; aux < auxCount; aux += 1) {
        if (auxAt + 16 > body.length) throw new Error('ELF version aux record is outside its section');
        const name = cString(table, body.readUInt32LE(auxAt + 8));
        const match = /^GLIBC_(\d+)\.(\d+)(?:\.(\d+))?$/.exec(name);
        if (match) found.push(match[0]);
        const next = body.readUInt32LE(auxAt + 12);
        if (next === 0) break;
        auxAt += next;
      }
      const next = body.readUInt32LE(needAt + 12);
      if (next === 0) break;
      needAt += next;
    }
  }
  return found;
}

function versionParts(value) {
  const match = /^(\d+)\.(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match) throw new Error(`Not a version: ${value}`);
  return [Number(match[1]), Number(match[2]), match[3] ? Number(match[3]) : 0];
}

function versionHigher(value, limit) {
  const left = versionParts(value.slice('GLIBC_'.length));
  const right = versionParts(limit);
  for (let i = 0; i < 3; i += 1) {
    if (left[i] !== right[i]) return left[i] > right[i];
  }
  return false;
}

function highest(versions) {
  return versions.reduce((best, version) => (versionHigher(version, best.slice('GLIBC_'.length)) ? version : best));
}

function check(file, glibcLimit) {
  const id = idFromName(file);
  const expected = id ? EXPECTED[id] : undefined;
  if (!expected) throw new Error(`No architecture is known for ${path.basename(file)}`);
  const buf = fs.readFileSync(file);
  const actual = identify(buf);
  if (!actual) throw new Error(`${file} is not an ELF, Mach-O, or PE file`);
  const sameFormat = actual.format === expected.format;
  const sameArch =
    expected.format === 'macho' ? actual.cpu === expected.cpu : actual.machine === expected.machine;
  if (!sameFormat || !sameArch) {
    const found =
      actual.format === 'macho'
        ? `Mach-O cpu 0x${actual.cpu.toString(16)}`
        : `${actual.format} machine 0x${actual.machine.toString(16)}`;
    throw new Error(`${path.basename(file)} is ${found}, not ${id}`);
  }
  if (glibcLimit === undefined) return `ok ${id}`;
  if (expected.format !== 'elf') throw new Error(`--glibc applies to Linux binaries, not ${id}`);
  const versions = glibcVersions(buf);
  if (versions.length === 0) throw new Error(`${path.basename(file)} does not require glibc`);
  const over = versions.filter((version) => versionHigher(version, glibcLimit));
  if (over.length > 0) {
    throw new Error(
      `${path.basename(file)} requires ${highest(over)}, above the glibc ${glibcLimit} floor`,
    );
  }
  return `ok ${id}, glibc ${highest(versions).slice('GLIBC_'.length)}`;
}

function elfWith(machine, versions) {
  const names = versions.map((version) => `GLIBC_${version}`);
  const bytes = [0];
  const offsets = names.map((name) => {
    const at = bytes.length;
    for (const code of Buffer.from(`${name}\0`, 'latin1')) bytes.push(code);
    return at;
  });
  const strings = Buffer.from(bytes);
  const aux = Buffer.alloc(16 * names.length);
  for (let i = 0; i < names.length; i += 1) {
    const at = i * 16;
    aux.writeUInt16LE(i + 1, at + 6);
    aux.writeUInt32LE(offsets[i], at + 8);
    aux.writeUInt32LE(i + 1 === names.length ? 0 : 16, at + 12);
  }
  const need = Buffer.alloc(16);
  need.writeUInt16LE(1, 0);
  need.writeUInt16LE(names.length, 2);
  need.writeUInt32LE(16, 8);
  const versionsSection = Buffer.concat([need, aux]);
  const namesSection = Buffer.from('\0.dynstr\0.gnu.version_r\0.shstrtab\0', 'latin1');

  const header = 64;
  const stringsAt = header;
  const versionsAt = stringsAt + strings.length;
  const namesAt = versionsAt + versionsSection.length;
  const headersAt = namesAt + namesSection.length;
  const file = Buffer.alloc(headersAt + 4 * 64);
  file[0] = 0x7f;
  file[1] = 0x45;
  file[2] = 0x4c;
  file[3] = 0x46;
  file[4] = 2;
  file[5] = 1;
  file.writeUInt16LE(machine, 18);
  file.writeBigUInt64LE(BigInt(headersAt), 40);
  file.writeUInt16LE(64, 52);
  file.writeUInt16LE(64, 58);
  file.writeUInt16LE(4, 60);
  file.writeUInt16LE(3, 62);
  strings.copy(file, stringsAt);
  versionsSection.copy(file, versionsAt);
  namesSection.copy(file, namesAt);

  function writeHeader(index, nameAt, type, offset, size, link) {
    const at = headersAt + index * 64;
    file.writeUInt32LE(nameAt, at);
    file.writeUInt32LE(type, at + 4);
    file.writeBigUInt64LE(BigInt(offset), at + 24);
    file.writeBigUInt64LE(BigInt(size), at + 32);
    file.writeUInt32LE(link, at + 40);
  }
  writeHeader(1, 1, ELF_STRTAB, stringsAt, strings.length, 0);
  writeHeader(2, 9, ELF_VERNEED, versionsAt, versionsSection.length, 1);
  writeHeader(3, 24, ELF_STRTAB, namesAt, namesSection.length, 0);
  return file;
}

function macho(cpu) {
  const file = Buffer.alloc(16);
  file.writeUInt32LE(0xfeedfacf, 0);
  file.writeUInt32LE(cpu, 4);
  return file;
}

function pe(machine) {
  const file = Buffer.alloc(0x80);
  file[0] = 0x4d;
  file[1] = 0x5a;
  file.writeUInt32LE(0x40, 0x3c);
  file.writeUInt32LE(0x4550, 0x40);
  file.writeUInt16LE(machine, 0x44);
  return file;
}

function assertThrows(fn, label) {
  try {
    fn();
  } catch {
    return;
  }
  throw new Error(`expected a failure: ${label}`);
}

function selfTest() {
  const os = require('node:os');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'djusb-node-check-'));
  const cases = [
    ['djusb-node.linux-x64-gnu.node', elfWith(0x3e, ['2.2.5', '2.17']), '2.17', true],
    ['djusb-node.linux-arm64-gnu.node', elfWith(0xb7, ['2.17', '2.28']), '2.17', false],
    ['djusb-node.linux-x64-gnu.node', elfWith(0xb7, ['2.17']), '2.17', false],
    ['djusb-node.darwin-x64.node', macho(0x01000007), undefined, true],
    ['djusb-node.darwin-arm64.node', macho(0x0100000c), undefined, true],
    ['djusb-node.win32-x64-msvc.node', pe(0x8664), undefined, true],
    ['djusb-node.win32-arm64-msvc.node', pe(0xaa64), undefined, true],
    ['djusb-node.win32-x64-msvc.node', pe(0xaa64), undefined, false],
  ];
  try {
    for (const [name, body, glibc, ok] of cases) {
      const file = path.join(dir, name);
      fs.writeFileSync(file, body);
      if (ok) check(file, glibc);
      else assertThrows(() => check(file, glibc), name);
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

const args = process.argv.slice(2);
if (args[0] === '--self-test') {
  try {
    selfTest();
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
  console.log('ok');
} else if (args.length === 1 || (args.length === 3 && args[1] === '--glibc')) {
  try {
    console.log(check(args[0], args[2]));
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error));
  }
} else {
  fail('Usage: node check-binary.cjs <djusb-node file> [--glibc 2.17] | --self-test');
}
