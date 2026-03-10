/**
 * Calypso Frontend Server
 *
 * Serves pre-built static files from /app/dist.
 * The bundle is baked into the image at build time — there is no runtime
 * artifact fetching, no update endpoint, and no external dependencies.
 * All release lifecycle is managed by the container orchestrator.
 */

const PORT = parseInt(process.env.PORT ?? "8080");
const DIST_DIR = "/app/dist";
const RELEASE_TAG = process.env.RELEASE_TAG ?? "unknown";

const server = Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);

    if (url.pathname === "/health") {
      return Response.json({ status: "ok", tag: RELEASE_TAG });
    }

    // Resolve path — default to index.html for SPA routing
    let filePath = `${DIST_DIR}${url.pathname}`;
    let file = Bun.file(filePath);

    if (!(await file.exists())) {
      file = Bun.file(`${DIST_DIR}/index.html`);
    }

    if (!(await file.exists())) {
      return new Response("Not Found", { status: 404 });
    }

    return new Response(file);
  },
});

console.log(`[frontend] listening on :${PORT} — release: ${RELEASE_TAG}`);
