const https = require("https");
const fs = require("fs");
const path = require("path");
const os = require("os");
const { execSync } = require("child_process");

const VERSION = require("./package.json").version;
const REPO = "risingwavelabs/box0";

function getPlatformKey() {
  const platform = os.platform();
  const arch = os.arch();

  const map = {
    "darwin-arm64": "darwin-arm64",
    "darwin-x64": "darwin-x64",
    "linux-x64": "linux-x64",
    "linux-arm64": "linux-arm64",
    "win32-x64": "windows-x64",
  };

  const key = `${platform}-${arch}`;
  if (!map[key]) {
    console.error(`Unsupported platform: ${key}`);
    console.error(`Supported: ${Object.keys(map).join(", ")}`);
    process.exit(1);
  }
  return map[key];
}

function getBinaryName() {
  return os.platform() === "win32" ? "b0.exe" : "b0";
}

async function download(url, dest) {
  return new Promise((resolve, reject) => {
    const follow = (url) => {
      https.get(url, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          follow(res.headers.location);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`Download failed: HTTP ${res.statusCode} from ${url}`));
          return;
        }
        const file = fs.createWriteStream(dest);
        res.pipe(file);
        file.on("finish", () => {
          file.close();
          resolve();
        });
      }).on("error", reject);
    };
    follow(url);
  });
}

async function main() {
  const platformKey = getPlatformKey();
  const binaryName = getBinaryName();
  const destPath = path.join(__dirname, "bin", binaryName);


  const ext = os.platform() === "win32" ? ".exe" : "";
  const assetName = `b0-${platformKey}${ext}`;
  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${assetName}`;

  console.log(`Downloading Box0 v${VERSION} for ${platformKey}...`);

  try {
    await download(url, destPath);
    if (os.platform() !== "win32") {
      fs.chmodSync(destPath, 0o755);
    }
    console.log("Box0 installed successfully.");
  } catch (e) {
    console.error(`Failed to download Box0: ${e.message}`);
    console.error(`URL: ${url}`);
    console.error(`\nYou can build from source instead:`);
    console.error(`  git clone https://github.com/${REPO}.git`);
    console.error(`  cd box0 && cargo build --release`);
    // Don't exit with error - let npm install succeed
    // The binary wrapper will show a helpful error when run
  }
}

main();
