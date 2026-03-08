// Reads GIT_BRAIN_METADATA JSON from stdin and validates the schema.
// Exits 1 with a descriptive error if validation fails.

const REQUIRED = ["retroactive_prompt", "outcome", "context", "agent", "session"];

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const raw = chunks.join("").trim();

if (!raw) {
  process.stderr.write("GIT_BRAIN_METADATA: JSON block is empty.\n");
  process.exit(1);
}

let metadata;
try {
  metadata = JSON.parse(raw);
} catch (e) {
  process.stderr.write(`GIT_BRAIN_METADATA: JSON parse error — ${e.message}\n`);
  process.stderr.write("Ensure the block is valid JSON with no trailing commas.\n");
  process.exit(1);
}

const missing = REQUIRED.filter(f => !metadata[f] || String(metadata[f]).trim() === "");
if (missing.length > 0) {
  process.stderr.write(`GIT_BRAIN_METADATA: Missing or empty required fields: ${missing.join(", ")}\n`);
  REQUIRED.forEach(f => process.stderr.write(`  - ${f}\n`));
  process.exit(1);
}

const rp = metadata.retroactive_prompt.trim();
if (rp.length < 50) {
  process.stderr.write("GIT_BRAIN_METADATA: retroactive_prompt is too short (minimum 50 characters).\n");
  process.stderr.write("It must be specific enough for another agent to reproduce this change.\n");
  process.exit(1);
}
