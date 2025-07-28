// frontend/copy-static.js
const fs = require('fs');
const path = require('path');

const sourceDir = path.join(__dirname, 'public'); // frontend/public
const destDir = path.join(__dirname, '..', 'public'); // ../public

console.log(`Copying static assets from ${sourceDir} to ${destDir}`);

// Create destination directory if it doesn't exist
if (!fs.existsSync(destDir)) {
  fs.mkdirSync(destDir, { recursive: true });
}

fs.readdir(sourceDir, (err, files) => {
  if (err) {
    console.error('Error reading source directory:', err);
    process.exit(1);
  }

  files.forEach(file => {
    const sourcePath = path.join(sourceDir, file);
    const destPath = path.join(destDir, file);

    fs.copyFile(sourcePath, destPath, (err) => {
      if (err) {
        console.error(`Error copying ${file}:`, err);
        process.exit(1);
      } else {
        console.log(`Copied: ${file}`);
      }
    });
  });
});