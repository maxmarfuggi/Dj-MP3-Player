// The installers ship the Rust core. Fail before packaging if it was not built.
try {
  const binding = require('djusb-node');
  if (typeof binding.LibraryManager !== 'function' || typeof binding.readVolume !== 'function') {
    throw new Error('djusb-node is missing LibraryManager or readVolume.');
  }
} catch (error) {
  console.error('The library core is not built. Run npm run build:node from the repository root.');
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
}
