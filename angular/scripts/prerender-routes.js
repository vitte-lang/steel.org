const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..', '..', 'site', 'browser');
const indexPath = path.join(root, 'index.html');

if (!fs.existsSync(indexPath)) {
  console.error('Missing index.html at', indexPath);
  process.exit(1);
}

const html = fs.readFileSync(indexPath, 'utf8');
const routes = [
  'downloads',
  'docs',
  'community',
  'blog',
  'showcase',
  'security',
  'status',
  'admin',
  'blog/steel-command-stays-minimal',
  'blog/multi-language-pattern-that-scales',
  'blog/release-notes-that-matter',
  'blog/steel-command-reste-minimal',
  'blog/pattern-multi-langage',
  'blog/notes-de-release',
  'blog/steel-command-bleibt-minimal',
  'blog/multi-language-muster',
  'blog/release-notizen',
  'blog/steel-command-resta-minimale',
  'blog/pattern-multi-lingua',
  'blog/note-di-release'
];

function adjustAssets(input) {
  let out = input.replace('<base href="./">', '<base href="../">');
  out = out.replace(/href="styles-/g, 'href="../styles-');
  out = out.replace(/src="main-/g, 'src="../main-');
  out = out.replace(/href="favicon.ico"/g, 'href="../favicon.ico"');
  return out;
}

for (const route of routes) {
  const dir = path.join(root, route);
  fs.mkdirSync(dir, { recursive: true });
  const outPath = path.join(dir, 'index.html');
  fs.writeFileSync(outPath, adjustAssets(html));
}

console.log('Static route entrypoints generated:', routes.join(', '));
