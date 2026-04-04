/* Copy web assets to an isolated folder for Tauri */
const fs = require('fs');
const path = require('path');

function copyRecursive(src, dest) {
    if (!fs.existsSync(src)) return;
    const stat = fs.statSync(src);
    if (stat.isDirectory()) {
        if (!fs.existsSync(dest)) fs.mkdirSync(dest, { recursive: true });
        for (const entry of fs.readdirSync(src)) {
            copyRecursive(path.join(src, entry), path.join(dest, entry));
        }
    } else {
        const dir = path.dirname(dest);
        if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
        fs.copyFileSync(src, dest);
    }
}

// src-tauri cwd -> project root is one level up
const projectRoot = path.resolve(__dirname, '..');
const outDir = path.join(projectRoot, 'dist-web');

if (fs.existsSync(outDir)) {
    fs.rmSync(outDir, { recursive: true, force: true });
}
fs.mkdirSync(outDir, { recursive: true });

const includeFiles = [
    'login.html',
    'settings.html',
];
const includeDirs = [
    'css',
    'js',
];

for (const f of includeFiles) {
    copyRecursive(path.join(projectRoot, f), path.join(outDir, f));
}
for (const d of includeDirs) {
    copyRecursive(path.join(projectRoot, d), path.join(outDir, d));
}

console.log('Prepared dist-web for Tauri:', outDir);

// Keep the icon source of truth under src-tauri/icons.
const iconsDir = path.join(__dirname, 'icons');
for (const iconName of ['icon.png', 'icon.icns', 'icon.ico']) {
    const iconPath = path.join(iconsDir, iconName);
    if (!fs.existsSync(iconPath)) {
        throw new Error(`Missing required Tauri icon: ${iconPath}`);
    }
}
console.log('Verified Tauri icons in', iconsDir);
