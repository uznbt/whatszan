const fs = require('fs');
const path = require('path');

exports.default = async function(context) {
  const sandboxPath = path.join(context.appOutDir, 'chrome-sandbox');
  if (!fs.existsSync(sandboxPath)) {
    fs.writeFileSync(sandboxPath, '');
    fs.chmodSync(sandboxPath, 0o4755);
    console.log(`[afterPack] Created dummy chrome-sandbox at ${sandboxPath}`);
  }
};
