import { resolve } from "node:path";

const repo = resolve(import.meta.dir, "..");
const target = resolve(repo, "target");
const wasmTarget = "wasm32-unknown-unknown";
const release = resolve(target, wasmTarget, "release");
const generated = resolve(import.meta.dir, "src/generated");

const lockfile = Bun.TOML.parse(await Bun.file(resolve(repo, "Cargo.lock")).text());
const { version } = lockfile.package.find((pkg) => pkg.name === "wasm-bindgen");
const bindgen = Bun.which("wasm-bindgen");
if (!bindgen || Bun.spawnSync([bindgen, "--version"]).stdout.toString().trim() !== `wasm-bindgen ${version}`) {
  console.error(`Install the wasm-bindgen CLI version in Cargo.lock:\ncargo install wasm-bindgen-cli --version ${version} --locked --force`);
  process.exit(1);
}

function run(command) {
  const result = Bun.spawnSync(command, { cwd: repo, stdout: "inherit", stderr: "inherit" });
  if (!result.success) process.exit(result.exitCode || 1);
}

run([
  "cargo", "build", "--locked", "--release", "--target", wasmTarget, "--target-dir", target,
  "-p", "gpdoc-playground", "--lib",
]);
run([
  bindgen, "--target", "web", "--out-dir", generated,
  "--out-name", "gpdoc-editor", resolve(release, "gpdoc_playground.wasm"),
]);
