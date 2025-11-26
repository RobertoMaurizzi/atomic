// scripts/build-release.js
import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = path.resolve(__dirname, '..');
const TAURI_DIR = path.join(PROJECT_ROOT, 'src-tauri');

// Simple logging
function log(msg) {
  console.log(msg);
}

function error(msg) {
  console.error(`ERROR: ${msg}`);
  process.exit(1);
}

// Parse semver
function parseVersion(versionStr) {
  const match = versionStr.match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!match) return null;
  return {
    major: parseInt(match[1]),
    minor: parseInt(match[2]),
    patch: parseInt(match[3])
  };
}

function formatVersion(v) {
  return `${v.major}.${v.minor}.${v.patch}`;
}

function bumpVersion(versionStr, type) {
  const v = parseVersion(versionStr);
  if (!v) error(`Invalid version: ${versionStr}`);

  if (type === 'major') {
    v.major++;
    v.minor = 0;
    v.patch = 0;
  } else if (type === 'minor') {
    v.minor++;
    v.patch = 0;
  } else if (type === 'patch') {
    v.patch++;
  }

  return formatVersion(v);
}

// Read current version from tauri.conf.json
function getCurrentVersion() {
  const configPath = path.join(TAURI_DIR, 'tauri.conf.json');
  const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
  return config.version;
}

// Update version in all files
function updateVersion(newVersion) {
  log(`Updating version to ${newVersion}...`);

  // Update tauri.conf.json
  const tauriConfigPath = path.join(TAURI_DIR, 'tauri.conf.json');
  const tauriConfig = JSON.parse(fs.readFileSync(tauriConfigPath, 'utf8'));
  tauriConfig.version = newVersion;
  fs.writeFileSync(tauriConfigPath, JSON.stringify(tauriConfig, null, 2) + '\n');

  // Update Cargo.toml
  const cargoTomlPath = path.join(TAURI_DIR, 'Cargo.toml');
  let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
  cargoToml = cargoToml.replace(/^version = ".*"$/m, `version = "${newVersion}"`);
  fs.writeFileSync(cargoTomlPath, cargoToml);

  // Update package.json
  const packageJsonPath = path.join(PROJECT_ROOT, 'package.json');
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
  packageJson.version = newVersion;
  fs.writeFileSync(packageJsonPath, JSON.stringify(packageJson, null, 2) + '\n');

  log(`✓ Updated version in all files`);
}

// Clean build artifacts
function clean() {
  log('Cleaning build artifacts...');

  const dirsToClean = [
    path.join(PROJECT_ROOT, 'dist'),
    path.join(TAURI_DIR, 'target/universal-apple-darwin'),
    path.join(TAURI_DIR, 'target/aarch64-apple-darwin/release'),
    path.join(TAURI_DIR, 'target/x86_64-apple-darwin/release'),
  ];

  for (const dir of dirsToClean) {
    if (fs.existsSync(dir)) {
      fs.rmSync(dir, { recursive: true, force: true });
      log(`  Removed ${path.relative(PROJECT_ROOT, dir)}`);
    }
  }

  log('✓ Cleaned build artifacts');
}

// Build the app
function build() {
  log('Building universal binary...');

  try {
    // Tauri build handles frontend build via beforeBuildCommand
    execSync('npx tauri build --target universal-apple-darwin', {
      cwd: PROJECT_ROOT,
      stdio: 'inherit'
    });

    log('✓ Build completed');
  } catch (err) {
    error('Build failed');
  }
}

// Verify the output
function verify(version) {
  log('Verifying build artifacts...');

  const dmgPath = path.join(
    TAURI_DIR,
    'target/universal-apple-darwin/release/bundle/dmg',
    `Atomic_${version}_universal.dmg`
  );

  if (!fs.existsSync(dmgPath)) {
    error(`DMG not found at ${dmgPath}`);
  }

  const dmgSize = (fs.statSync(dmgPath).size / 1024 / 1024).toFixed(1);
  log(`✓ DMG created: ${dmgSize} MB`);
  log(`\nOutput: ${dmgPath}`);
}

// Git operations
function gitCommit(version) {
  log('Creating git commit...');

  try {
    execSync('git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml', {
      cwd: PROJECT_ROOT,
      stdio: 'inherit'
    });

    execSync(`git commit -m "Bump version to v${version}"`, {
      cwd: PROJECT_ROOT,
      stdio: 'inherit'
    });

    log('✓ Git commit created');
  } catch (err) {
    error('Git commit failed');
  }
}

function gitTag(version) {
  log('Creating git tag...');

  try {
    const tagName = `v${version}`;

    // Check if tag exists
    try {
      execSync(`git rev-parse ${tagName}`, { cwd: PROJECT_ROOT, stdio: 'pipe' });
      log(`⚠ Tag ${tagName} already exists, skipping`);
      return;
    } catch {
      // Tag doesn't exist, create it
    }

    execSync(`git tag -a ${tagName} -m "Release ${tagName}"`, {
      cwd: PROJECT_ROOT,
      stdio: 'inherit'
    });

    log(`✓ Git tag ${tagName} created`);
  } catch (err) {
    error('Git tag failed');
  }
}

// Show help
function showHelp() {
  console.log(`
Usage: node scripts/build-release.js [options]

Build macOS universal binary for Atomic

Version Management:
  --bump-patch              Bump patch version (0.2.0 → 0.2.1)
  --bump-minor              Bump minor version (0.2.0 → 0.3.0)
  --bump-major              Bump major version (0.2.0 → 1.0.0)

Build Options:
  --clean                   Clean build artifacts before building
  --no-build                Version bump only (skip build)

Git Integration:
  --commit                  Create git commit after version bump
  --tag                     Create git tag (implies --commit)

Utility:
  --help                    Show this help message

Examples:
  node scripts/build-release.js
  node scripts/build-release.js --bump-patch --clean
  node scripts/build-release.js --bump-patch --tag
  `);
}

// Main
async function main() {
  const args = process.argv.slice(2);

  // Parse arguments
  const options = {
    bumpType: null,
    clean: false,
    build: true,
    commit: false,
    tag: false,
    help: false
  };

  for (const arg of args) {
    if (arg === '--bump-patch') options.bumpType = 'patch';
    else if (arg === '--bump-minor') options.bumpType = 'minor';
    else if (arg === '--bump-major') options.bumpType = 'major';
    else if (arg === '--clean') options.clean = true;
    else if (arg === '--no-build') options.build = false;
    else if (arg === '--commit') options.commit = true;
    else if (arg === '--tag') {
      options.tag = true;
      options.commit = true; // Tag implies commit
    }
    else if (arg === '--help') options.help = true;
    else {
      error(`Unknown argument: ${arg}`);
    }
  }

  if (options.help) {
    showHelp();
    return;
  }

  log('Building Atomic release for macOS\n');

  // Get current version
  let version = getCurrentVersion();
  log(`Current version: ${version}`);

  // Handle version bumping
  if (options.bumpType) {
    version = bumpVersion(version, options.bumpType);
    updateVersion(version);
  }

  // Git commit (if version was bumped and commit requested)
  if (options.bumpType && options.commit) {
    gitCommit(version);
  }

  // Git tag (if requested)
  if (options.bumpType && options.tag) {
    gitTag(version);
  }

  // Build
  if (options.build) {
    if (options.clean) {
      clean();
    }

    build();
    verify(version);
  }

  log('\n✓ Done!');
}

main().catch(err => {
  console.error('ERROR:', err.message);
  process.exit(1);
});
