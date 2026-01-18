import { Hono } from "hono";
import { cors } from "hono/cors";
import { logger } from "hono/logger";

import { completeRouter } from "./routes/complete";
import { statusRouter } from "./routes/status";
import { feedbackRouter } from "./routes/feedback";
import { diffRouter } from "./routes/diff";
import { workersRouter } from "./routes/workers";
import { configRouter } from "./routes/config";

const app = new Hono();

// Middleware
app.use("*", logger());
app.use("*", cors({
  origin: "http://localhost:*",
  allowMethods: ["GET", "POST", "PUT", "DELETE"],
}));

// Health check
app.get("/", (c) => {
  return c.json({
    name: "Team Claude Coordination Server",
    version: "0.1.0",
    status: "running",
    timestamp: new Date().toISOString(),
  });
});

app.get("/health", (c) => {
  return c.json({
    status: "healthy",
    uptime: process.uptime(),
    timestamp: new Date().toISOString(),
  });
});

// Mount routers
app.route("/complete", completeRouter);
app.route("/status", statusRouter);
app.route("/feedback", feedbackRouter);
app.route("/diff", diffRouter);
app.route("/workers", workersRouter);
app.route("/config", configRouter);

// Error handler
app.onError((err, c) => {
  console.error("[server] Unhandled error:", err);
  return c.json({
    success: false,
    error: err.message || "Internal server error",
    timestamp: new Date().toISOString(),
  }, 500);
});

// 404 handler
app.notFound((c) => {
  return c.json({
    success: false,
    error: "Not found",
    timestamp: new Date().toISOString(),
  }, 404);
});

// Start server
const PORT = parseInt(process.env.PORT || "3847", 10);

console.log(`
╔══════════════════════════════════════════════════════════════╗
║           Team Claude Coordination Server                    ║
╠══════════════════════════════════════════════════════════════╣
║  Port: ${PORT.toString().padEnd(54)}║
║  Project: ${(process.env.PROJECT_ROOT || process.cwd()).slice(-50).padEnd(50)}║
╚══════════════════════════════════════════════════════════════╝
`);

console.log("[server] Starting server...");
console.log(`[server] Endpoints:`);
console.log(`  - GET  /           - Server info`);
console.log(`  - GET  /health     - Health check`);
console.log(`  - POST /complete   - Worker completion report`);
console.log(`  - GET  /status     - All workers status`);
console.log(`  - GET  /status/:id - Specific worker status`);
console.log(`  - POST /feedback   - Send feedback to worker`);
console.log(`  - GET  /diff/:id   - Get worktree diff summary`);
console.log(`  - GET  /workers    - List workers`);
console.log(`  - POST /workers    - Register worker`);
console.log(`  - GET  /config     - Get configuration`);
console.log(`  - POST /config/set - Set configuration value`);
console.log(`  - GET  /config/templates - List templates`);
console.log(`  - GET  /config/rules     - List review rules`);
console.log(``);

export default {
  port: PORT,
  fetch: app.fetch,
};
