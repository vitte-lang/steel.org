const fs = require('fs');
const path = require('path');

const root = path.join(__dirname, '..', '..', 'site', 'browser');
const pages = [
  '/',
  '/downloads/',
  '/docs/',
  '/community/',
  '/blog/',
  '/showcase/',
  '/security/',
  '/status/',
  '/admin/',
  '/blog/steel-command-stays-minimal/',
  '/blog/multi-language-pattern-that-scales/',
  '/blog/release-notes-that-matter/',
  '/blog/steel-command-reste-minimal/',
  '/blog/pattern-multi-langage/',
  '/blog/notes-de-release/',
  '/blog/steel-command-bleibt-minimal/',
  '/blog/multi-language-muster/',
  '/blog/release-notizen/',
  '/blog/steel-command-resta-minimale/',
  '/blog/pattern-multi-lingua/',
  '/blog/note-di-release/'
];

const baseUrl = 'https://example.netlify.app';

const xml = `<?xml version="1.0" encoding="UTF-8"?>\n` +
  `<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n` +
  pages
    .map((page) => {
      const loc = `${baseUrl}${page}`.replace(/\/\//g, '/').replace('https:/', 'https://');
      return `  <url><loc>${loc}</loc></url>`;
    })
    .join('\n') +
  `\n</urlset>\n`;

fs.writeFileSync(path.join(root, 'sitemap.xml'), xml);
console.log('sitemap.xml generated');
