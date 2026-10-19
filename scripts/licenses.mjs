// Generate a reviewable inventory from Cargo.lock and the actual crate license files.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const metadata = JSON.parse(
  execFileSync(
    process.env.FREECUT_CARGO || "cargo",
    [
      "metadata",
      "--manifest-path",
      path.join(root, "src-tauri/Cargo.toml"),
      "--format-version",
      "1",
      "--filter-platform",
      "x86_64-pc-windows-msvc",
      "--locked",
    ],
    { maxBuffer: 20 * 1024 * 1024, encoding: "utf8" },
  ),
);
const packages = metadata.packages
  .filter((pkg) => pkg.source)
  .sort((a, b) =>
    `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`),
  );
const texts = new Map();
const inventory = [];
function licenseText(filename) {
  return fs
    .readFileSync(filename, "utf8")
    .replace(/\r\n/g, "\n")
    .split("\n")
    .map((line) => line.trimEnd())
    .join("\n")
    .trim();
}
for (const pkg of packages) {
  const dir = path.dirname(pkg.manifest_path);
  const licenses = fs
    .readdirSync(dir, { withFileTypes: true })
    .filter(
      (entry) =>
        entry.isFile() &&
        /^(licen[cs]e|copying|notice)([.\-_]|$)/i.test(entry.name),
    );
  const references = [];
  for (const file of licenses) {
    const text = licenseText(path.join(dir, file.name));
    const hash = createHash("sha256").update(text).digest("hex").slice(0, 16);
    texts.set(hash, text);
    references.push(`${file.name}: ${hash}`);
  }
  if (!references.length && pkg.license_file) {
    const text = licenseText(path.resolve(dir, pkg.license_file));
    const hash = createHash("sha256").update(text).digest("hex").slice(0, 16);
    texts.set(hash, text);
    references.push(`${pkg.license_file}: ${hash}`);
  }
  inventory.push(
    `${pkg.name} ${pkg.version}\nLicense: ${pkg.license || "See license file"}\nSource: ${pkg.repository || pkg.source}\n${references.join("\n") || "License text: see upstream source at the version above."}`,
  );
}
const apiPackage = JSON.parse(
  fs.readFileSync(
    path.join(root, "node_modules/@tauri-apps/api/package.json"),
    "utf8",
  ),
);
const apiDir = path.join(root, "node_modules/@tauri-apps/api");
const apiTexts = fs
  .readdirSync(apiDir)
  .filter((name) => /^LICENSE/i.test(name))
  .map((name) => `${name}\n${fs.readFileSync(path.join(apiDir, name), "utf8")}`)
  .join("\n\n");
inventory.push(
  `@tauri-apps/api ${apiPackage.version}\nLicense: ${apiPackage.license}\nSource: https://github.com/tauri-apps/tauri\n\n${apiTexts}`,
);
const output = `FreeCut Tauri third-party licenses\nGenerated from Cargo.lock for Windows (runtime and build dependencies).\nIdentical license texts are deduplicated by their SHA-256 prefix.\n\n${inventory.join("\n\n")}\n\nFULL LICENSE TEXTS\n\n${[...texts].map(([hash, text]) => `=== ${hash} ===\n${text}`).join("\n\n")}\n`;
fs.writeFileSync(path.join(root, "src-tauri/THIRD_PARTY_LICENSES.txt"), output);
console.log(
  `Recorded ${packages.length} Rust packages and @tauri-apps/api; ${texts.size} distinct license texts.`,
);
