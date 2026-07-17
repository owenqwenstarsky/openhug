const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");
// pixelmatch v7 is ESM-only; require() returns the module namespace.
const pixelmatchModule = require("pixelmatch");
const pixelmatch = pixelmatchModule.default || pixelmatchModule;
const { PNG } = require("pngjs");

const outputRoot = path.resolve(__dirname, "../../previews");
const themes = ["light", "dark"];
fs.mkdirSync(outputRoot, { recursive: true });
for (const file of fs.readdirSync(outputRoot)) {
  if (/^\d{2}-.+\.png$/.test(file)) fs.rmSync(path.join(outputRoot, file));
}

const repos = [
  { id: "r1", owner: "atelier", kind: "model", name: "lumen-7b-instruct", description: "A concise, capable instruction model for everyday research workflows.", visibility: "public", head_commit_id: "c4e7b1f2d3a4", download_count: 12843, updated_at: "2026-07-15T14:40:00Z", files: [{ path: "README.md", sha256: "2a4ef67c21bc8fd445e0", size: 12430 }, { path: "config.json", sha256: "02ef7c66bd52ab85901e", size: 2184 }, { path: "model-00001-of-00003.safetensors", sha256: "ac9b57d018bb2c110b8c", size: 3221225472 }, { path: "tokenizer.json", sha256: "f8a4fd0218fe20bb9d1c", size: 1849032 }] },
  { id: "r2", owner: "northstar", kind: "model", name: "ember-vision-3b", description: "Vision-language foundation model tuned for product imagery and diagrams.", visibility: "public", head_commit_id: "a91dfe2098bb", download_count: 6970, updated_at: "2026-07-14T09:14:00Z", files: [] },
  { id: "r3", owner: "fieldnotes", kind: "model", name: "moss-embed-large", description: "Dense retrieval embeddings for long documents and technical corpora.", visibility: "private", head_commit_id: "91a80e11c5a2", download_count: 2130, updated_at: "2026-07-12T11:00:00Z", files: [] },
  { id: "r4", owner: "atelier", kind: "dataset", name: "canopy-captions-v2", description: "High-quality image captions with multilingual scene descriptions.", visibility: "public", head_commit_id: "f2d4c8ea1120", download_count: 8751, updated_at: "2026-07-15T16:20:00Z", files: [{ path: "README.md", sha256: "0aa5fe0c191a8435c211", size: 18320 }, { path: "data/train-00000.parquet", sha256: "cc15e90b9987c314f506", size: 842216704 }, { path: "data/validation.parquet", sha256: "c0f31022891adfd4579e", size: 49615032 }, { path: "licenses/source-attribution.csv", sha256: "71fd11bb1c2ecdda72a3", size: 80389 }] },
  { id: "r5", owner: "meridian", kind: "dataset", name: "tidepool-qa", description: "Specialist question-answer pairs for ocean science and climate literacy.", visibility: "public", head_commit_id: "b7e913a20d91", download_count: 4420, updated_at: "2026-07-13T18:10:00Z", files: [] },
  { id: "r6", owner: "fieldnotes", kind: "dataset", name: "archive-summaries", description: "Human-reviewed archival summaries, preserved with source metadata.", visibility: "private", head_commit_id: "91dd4011a4bd", download_count: 910, updated_at: "2026-07-10T13:05:00Z", files: [] },
];

async function mockApi(page, { signedOut = false, theme = "light" } = {}) {
  await page.route("**/api/v1/**", async (route) => {
    const url = new URL(route.request().url());
    const endpoint = url.pathname.replace("/api/v1", "");
    let body = {};
    if (endpoint === "/setup/status") body = { initialized: true, instance_name: "OpenHug", signup_policy: "immediate", default_visibility: "public" };
    else if (endpoint === "/auth/me" && signedOut) {
      await route.fulfill({ status: 401, contentType: "application/json", body: JSON.stringify({ error: "Not signed in" }) });
      return;
    } else if (endpoint === "/auth/me" && route.request().method() === "PUT") body = { id: "u1", username: "mara", email: "mara@openhug.studio", role: "superuser", theme };
    else if (endpoint === "/auth/me") body = { id: "u1", username: "mara", email: "mara@openhug.studio", role: "superuser", theme };
    else if (endpoint === "/repositories") {
      const kind = url.searchParams.get("kind");
      const search = (url.searchParams.get("search") || "").toLowerCase();
      body = repos.filter((repo) => repo.kind === kind && (!search || `${repo.owner}/${repo.name} ${repo.description}`.toLowerCase().includes(search)));
    } else if (/^\/repositories\/(model|dataset)\/[^/]+\/[^/]+$/.test(endpoint)) {
      const [, kind, owner, name] = endpoint.match(/^\/repositories\/(model|dataset)\/([^/]+)\/([^/]+)$/);
      body = repos.find((repo) => repo.kind === kind && repo.owner === owner && repo.name === name) || {};
    } else if (/\/commits$/.test(endpoint)) body = [
      { id: "c4e7b1f2d3a4769e", author: "mara", message: "Refine model card and evaluation notes", created_at: "2026-07-15T14:40:00Z" },
      { id: "8d99ac04291ba2ed", author: "eli", message: "Upload fp16 checkpoint shards", created_at: "2026-07-14T10:12:00Z" },
      { id: "2bd4f841a64bce1a", author: "mara", message: "Initial release", created_at: "2026-07-10T09:00:00Z" },
    ];
    else if (endpoint === "/tokens") body = [
      { id: "ohp_8e3f1a09", name: "MacBook Pro CLI", scopes: ["read", "write"], created_at: "2026-06-28T13:05:00Z", last_used_at: "2026-07-15T18:45:00Z" },
      { id: "ohp_4c9b62e7", name: "Training runner", scopes: ["read", "write"], created_at: "2026-06-11T09:20:00Z", last_used_at: "2026-07-14T08:05:00Z" },
    ];
    else if (endpoint === "/admin/settings") body = { instance_name: "OpenHug", signup_policy: "immediate", default_visibility: "public", retention_days: 30 };
    else if (endpoint === "/admin/users") body = [
      { id: "u1", username: "mara", email: "mara@openhug.studio", role: "superuser", status: "active" },
      { id: "u2", username: "eli", email: "eli@openhug.studio", role: "member", status: "active" },
      { id: "u3", username: "sam", email: "sam@openhug.studio", role: "member", status: "pending" },
    ];
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });
  });
}

async function applyTheme(page, theme) {
  await page.evaluate((themeName) => {
    document.documentElement.dataset.theme = themeName;
    document.documentElement.style.colorScheme = themeName;
  }, theme);
}

function screenshotsMatch(pathA, pathB) {
  const imgA = PNG.sync.read(fs.readFileSync(pathA));
  const imgB = PNG.sync.read(fs.readFileSync(pathB));
  if (imgA.width !== imgB.width || imgA.height !== imgB.height) return false;
  return pixelmatch(imgA.data, imgB.data, null, imgA.width, imgA.height, { threshold: 0 }) === 0;
}

async function capture(page, output, theme, url, name, action) {
  await page.goto("http://localhost:3000/", { waitUntil: "networkidle" });
  await page.evaluate((nextPath) => {
    history.pushState({}, "", nextPath);
    dispatchEvent(new PopStateEvent("popstate"));
  }, url);
  await page.waitForTimeout(250);
  if (action) await action();
  await applyTheme(page, theme);
  await page.evaluate(() => scrollTo(0, 0));
  await page.waitForTimeout(300);
  const finalPath = path.join(output, name);
  const tempPath = `${finalPath}.tmp.png`;
  await page.screenshot({
    path: tempPath,
    fullPage: true,
    animations: "disabled",
    caret: "hide",
  });
  if (fs.existsSync(finalPath) && screenshotsMatch(tempPath, finalPath)) {
    fs.rmSync(tempPath);
    return;
  }
  fs.renameSync(tempPath, finalPath);
}

async function captureTheme(browser, theme) {
  const output = path.join(outputRoot, theme);
  fs.mkdirSync(output, { recursive: true });

  const page = await browser.newPage({ viewport: { width: 1440, height: 1040 }, deviceScaleFactor: 1 });
  await mockApi(page, { theme });
  await capture(page, output, theme, "/models", "01-models.png");
  await capture(page, output, theme, "/datasets", "02-datasets.png");
  await capture(page, output, theme, "/atelier/lumen-7b-instruct", "03-model-repository.png");
  await capture(page, output, theme, "/datasets/atelier/canopy-captions-v2", "04-dataset-repository.png");
  await capture(page, output, theme, "/new/model", "05-new-model.png");
  await capture(page, output, theme, "/new/dataset", "06-new-dataset.png");
  await capture(page, output, theme, "/settings", "07-api-tokens.png");
  await capture(page, output, theme, "/settings", "08-administration.png", () => page.getByRole("button", { name: "Administration" }).click());
  await page.close();

  const authPage = await browser.newPage({ viewport: { width: 1440, height: 1040 }, deviceScaleFactor: 1 });
  await mockApi(authPage, { signedOut: true, theme });
  await capture(authPage, output, theme, "/", "09-login.png", async () => {
    await authPage.getByLabel("Email or username").fill("mara@openhug.studio");
    await authPage.getByLabel("Password").fill("studio-preview-only");
  });
  await capture(authPage, output, theme, "/", "10-signup.png", async () => {
    await authPage.getByRole("button", { name: "Need an account? Sign up" }).click();
    await authPage.getByLabel("Username").fill("jordan");
    await authPage.getByLabel("Email").fill("jordan@example.com");
    await authPage.getByLabel("Password").fill("studio-preview-only");
  });
  await authPage.close();
}

(async () => {
  const launchOptions = { headless: true };
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH) {
    launchOptions.executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
  }
  const browser = await chromium.launch(launchOptions);
  for (const theme of themes) await captureTheme(browser, theme);
  await browser.close();
  console.log(`Wrote ${themes.length * 10} previews to ${outputRoot}`);
})();
