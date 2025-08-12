// frontend/copy-static.js
const fs = require("fs");
const path = require("path");

const sourceDir = path.join(__dirname, "public"); // frontend/public
const destDir = path.join(__dirname, "..", "public"); // ../public

console.log(`Copying static assets from ${sourceDir} to ${destDir}`);

// Recursively copy directory
function copyRecursive(src, dest) {
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(dest, { recursive: true });
  }

  fs.readdirSync(src, { withFileTypes: true }).forEach((entry) => {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      copyRecursive(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
      console.log(`Copied: ${path.relative(sourceDir, srcPath)}`);
    }
  });
}

copyRecursive(sourceDir, destDir);
