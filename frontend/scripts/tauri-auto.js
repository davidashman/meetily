#!/usr/bin/env node
/**
 * Auto-detect GPU and run Tauri with appropriate features
 */

const { execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');

// Get the command (dev or build)
const command = process.argv[2];
if (!command || !['dev', 'build'].includes(command)) {
  console.error('Usage: node tauri-auto.js [dev|build]');
  process.exit(1);
}

// Detect GPU feature
let feature = '';

// Check for environment variable override first
if (process.env.TAURI_GPU_FEATURE) {
  feature = process.env.TAURI_GPU_FEATURE;
  console.log(`🔧 Using forced GPU feature from environment: ${feature}`);
} else {
  try {
    const result = execSync('node scripts/auto-detect-gpu.js', {
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'inherit']
    });
    feature = result.trim();
  } catch (err) {
    // If detection fails, continue with no features
  }
}

console.log(''); // Empty line for spacing

// Platform-specific environment variables
const platform = os.platform();
const env = { ...process.env };

if (platform === 'linux' && feature === 'cuda') {
  console.log('🐧 Linux/CUDA detected: Setting CMAKE flags for NVIDIA GPU');
  env.CMAKE_CUDA_ARCHITECTURES = '75';
  env.CMAKE_CUDA_STANDARD = '17';
  env.CMAKE_POSITION_INDEPENDENT_CODE = 'ON';
}

// Build the tauri command
let tauriCmd = `tauri ${command}`;
if (feature && feature !== 'none') {
  tauriCmd += ` -- --features ${feature}`;
  console.log(`🚀 Running: tauri ${command} with features: ${feature}`);
} else {
  console.log(`🚀 Running: tauri ${command} (CPU-only mode)`);
}
console.log('');

// Execute the command
try {
  execSync(tauriCmd, { stdio: 'inherit', env });
} catch (err) {
  process.exit(err.status || 1);
}

// Re-sign the macOS bundle with a stable identifier.
// The linker-generated adhoc signature uses a hash-suffixed identifier (e.g.
// `meetily-2dbe4a5ed93c9bc6`) that changes every build. macOS's TCC database
// keys permissions on that identifier, so without this step users get
// re-prompted for mic/notifications on every rebuild.
if (command === 'build' && platform === 'darwin') {
  const appPath = path.resolve(__dirname, '..', '..', 'target', 'release', 'bundle', 'macos', 'meetily.app');
  const entitlementsPath = path.resolve(__dirname, '..', 'src-tauri', 'entitlements.plist');

  if (fs.existsSync(appPath)) {
    console.log('');
    console.log(`🔏 Re-signing ${appPath} with stable identifier (com.meetily.ai)`);
    try {
      execSync(
        `codesign --force --sign - --identifier com.meetily.ai ` +
          `--entitlements "${entitlementsPath}" --options runtime --deep ` +
          `"${appPath}"`,
        { stdio: 'inherit' }
      );
      console.log('✅ Re-sign complete');
    } catch (err) {
      console.error('⚠️  codesign re-sign failed:', err.message);
      process.exit(err.status || 1);
    }
  } else {
    console.warn(`⚠️  Skipping re-sign: bundle not found at ${appPath}`);
  }
}
