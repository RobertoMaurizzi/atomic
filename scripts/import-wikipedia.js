// scripts/import-wikipedia.js
import Database from 'better-sqlite3';
import { randomUUID } from 'crypto';

const WIKIPEDIA_API = 'https://en.wikipedia.org/api/rest_v1/page/summary/';

// Database path - in development, the database is in the Tauri app data directory
// For macOS: ~/Library/Application Support/com.atomic.app/atomic.db
// For Linux: ~/.local/share/com.atomic.app/atomic.db
// For Windows: %APPDATA%/com.atomic.app/atomic.db
function getDefaultDbPath() {
  const platform = process.platform;
  const home = process.env.HOME || process.env.USERPROFILE;
  
  if (platform === 'darwin') {
    return `${home}/Library/Application Support/com.atomic.app/atomic.db`;
  } else if (platform === 'linux') {
    return `${home}/.local/share/com.atomic.app/atomic.db`;
  } else if (platform === 'win32') {
    return `${process.env.APPDATA}/com.atomic.app/atomic.db`;
  }
  throw new Error(`Unsupported platform: ${platform}`);
}

// Seed articles for each domain
const SEEDS = {
  computing: [
    'History_of_computing',
    'Computer',
    'Alan_Turing',
    'Programming_language',
    'Artificial_intelligence',
    'Internet',
    'World_Wide_Web',
    'Operating_system',
    'Algorithm',
    'Data_structure',
    'Software_engineering',
    'Computer_science',
    'Machine_learning',
    'Database',
    'Computer_network',
  ],
  philosophy: [
    'Philosophy',
    'Epistemology',
    'Ethics',
    'Metaphysics',
    'Logic',
    'Plato',
    'Aristotle',
    'Immanuel_Kant',
    'Friedrich_Nietzsche',
    'Existentialism',
    'Stoicism',
    'Utilitarianism',
    'Philosophy_of_mind',
    'Political_philosophy',
    'Aesthetics',
  ],
  history: [
    'European_Union',
    'History_of_Europe',
    'Ancient_Greece',
    'Roman_Empire',
    'Renaissance',
    'World_War_I',
    'World_War_II',
    'Cold_War',
    'French_Revolution',
    'Industrial_Revolution',
    'Byzantine_Empire',
    'Holy_Roman_Empire',
    'Napoleonic_Wars',
    'Ancient_Rome',
    'Medieval_Europe',
  ],
};

async function fetchArticle(title) {
  try {
    const response = await fetch(`${WIKIPEDIA_API}${encodeURIComponent(title)}`);
    if (!response.ok) return null;

    const data = await response.json();

    // Skip disambiguation pages and missing articles
    if (data.type === 'disambiguation' || !data.extract) return null;

    return {
      title: data.title,
      content: data.extract,
      url: data.content_urls?.desktop?.page || `https://en.wikipedia.org/wiki/${title}`,
    };
  } catch (error) {
    console.error(`Failed to fetch ${title}:`, error.message);
    return null;
  }
}

async function fetchLinksFromArticle(title, limit = 10) {
  try {
    // Use MediaWiki Action API to get links from the article
    const response = await fetch(
      `https://en.wikipedia.org/w/api.php?action=query&titles=${encodeURIComponent(title)}&prop=links&pllimit=${limit}&plnamespace=0&format=json&origin=*`
    );
    if (!response.ok) return [];

    const data = await response.json();
    const pages = data.query?.pages;
    if (!pages) return [];

    // Get the first (and only) page
    const pageId = Object.keys(pages)[0];
    const links = pages[pageId]?.links || [];

    return links.map((link) => link.title.replace(/ /g, '_'));
  } catch (error) {
    console.error(`  Error fetching links from ${title}:`, error.message);
    return [];
  }
}

async function importArticles(db, maxArticles = 500) {
  const imported = new Set();
  const queue = [];

  // Add all seeds to queue
  for (const [domain, seeds] of Object.entries(SEEDS)) {
    for (const seed of seeds) {
      queue.push({ title: seed, domain });
    }
  }

  const insertAtom = db.prepare(`
    INSERT INTO atoms (id, content, source_url, created_at, updated_at, embedding_status)
    VALUES (?, ?, ?, ?, ?, 'pending')
  `);

  let count = 0;

  while (queue.length > 0 && count < maxArticles) {
    const { title, domain } = queue.shift();

    if (imported.has(title)) continue;

    const article = await fetchArticle(title);
    if (!article || article.content.length < 20) {
      console.log(`  Skipped: ${title} (${article ? 'too short: ' + article.content.length + ' chars' : 'fetch failed'})`);
      continue;
    }

    // Insert into database
    const now = new Date().toISOString();
    const id = randomUUID();

    try {
      insertAtom.run(
        id,
        `# ${article.title}\n\n${article.content}`,
        article.url,
        now,
        now
      );

      imported.add(title);
      count++;

      // Fetch links from this article and add to queue
      if (count < maxArticles) {
        const links = await fetchLinksFromArticle(title, 10);
        let addedCount = 0;
        for (const linkedTitle of links) {
          if (!imported.has(linkedTitle)) {
            queue.push({ title: linkedTitle, domain });
            addedCount++;
          }
        }
        console.log(`[${count}/${maxArticles}] Imported: ${article.title} (${domain}) | +${addedCount} linked | Queue: ${queue.length}`);
      } else {
        console.log(`[${count}/${maxArticles}] Imported: ${article.title} (${domain})`);
      }

      // Rate limiting - be nice to Wikipedia
      await new Promise((resolve) => setTimeout(resolve, 100));
    } catch (error) {
      console.error(`Failed to insert ${article.title}:`, error.message);
    }
  }

  if (count >= maxArticles) {
    console.log(`\nReached target of ${maxArticles} articles.`);
  } else if (queue.length === 0) {
    console.log(`\nQueue depleted after ${count} articles.`);
    console.log(`This might indicate that related articles aren't being fetched properly.`);
  }

  console.log(`\nImported ${count} articles successfully.`);
  console.log(`\nNext steps:`);
  console.log(`1. Start the Atomic app`);
  console.log(`2. Embeddings will process automatically in the background`);
  console.log(`3. Watch atoms update with tags as processing completes`);
  console.log(`\nThis may take 10-30 minutes for large batches depending on API rate limits.`);
  return count;
}

async function main() {
  const args = process.argv.slice(2);
  let maxArticles = 500;
  let dbPath = null;

  // Parse arguments
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--db' && args[i + 1]) {
      dbPath = args[i + 1];
      i++;
    } else if (!isNaN(parseInt(args[i]))) {
      maxArticles = parseInt(args[i]);
    }
  }

  // Use default path if not specified
  if (!dbPath) {
    dbPath = getDefaultDbPath();
  }

  console.log(`Opening database at ${dbPath}`);
  
  // Check if database exists
  const fs = await import('fs');
  if (!fs.existsSync(dbPath)) {
    console.error(`\nError: Database not found at ${dbPath}`);
    console.error('\nThe database is created when you first run the Atomic app.');
    console.error('Please run the app at least once before using this import script.');
    console.error('\nAlternatively, specify a custom database path with --db <path>');
    process.exit(1);
  }

  const db = new Database(dbPath);

  console.log(`Importing up to ${maxArticles} Wikipedia articles...\n`);

  await importArticles(db, maxArticles);

  db.close();
  console.log('\nDone! Start the app to trigger embedding and tag extraction.');
  console.log('Note: Processing many atoms may take a while depending on your API rate limits.');
}

main().catch(console.error);

