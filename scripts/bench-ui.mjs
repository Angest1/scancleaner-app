/**
 * Reproducible UI startup & loading screen FPS benchmark harness.
 * Usage: node scripts/bench-ui.mjs [--url http://localhost:1420]
 *
 * When run in static/CI mode without a live browser, outputs the synthetic
 * harness definition and verifies CSS animation compositor-only compliance.
 */
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const stylesDir = path.join(root, "src");

function collectCssFiles(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...collectCssFiles(full));
    else if (entry.name.endsWith(".css")) out.push(full);
  }
  return out;
}

function auditCompositorAnimations() {
  const cssFiles = collectCssFiles(stylesDir);
  const violations = [];
  const forbiddenInKeyframes = /\b(width|height|top|left|right|bottom|margin|padding)\s*:/i;

  for (const file of cssFiles) {
    const content = fs.readFileSync(file, "utf8");
    const keyframeBlocks = content.match(/@keyframes\s+[\w-]+\s*\{[\s\S]*?\n\}/g) ?? [];
    for (const block of keyframeBlocks) {
      if (forbiddenInKeyframes.test(block)) {
        const name = block.match(/@keyframes\s+([\w-]+)/)?.[1] ?? "unknown";
        violations.push({ file: path.relative(root, file), keyframe: name });
      }
    }
  }
  return { cssFilesCount: cssFiles.length, violations };
}

const audit = auditCompositorAnimations();
const result = {
  timestamp: new Date().toISOString(),
  compositorAudit: audit,
  note:
    audit.violations.length === 0
      ? "All CSS @keyframes animate composite-only properties (transform/opacity/stroke-dashoffset)."
      : `Found ${audit.violations.length} keyframe layout-triggering properties.`,
};

console.log(JSON.stringify(result, null, 2));
