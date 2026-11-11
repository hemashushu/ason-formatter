import { spawnSync } from "node:child_process";
import { mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = resolve(root, "Cargo.toml");
const version = "0.2.130";
const hash = (data) => createHash("sha256").update(data).digest("hex");
function run(command, args, capture = false) {
    const result = spawnSync(command, args, {
        cwd: root,
        encoding: "utf8",
        stdio: capture ? "pipe" : "inherit",
    });
    if (result.error || result.status !== 0)
        throw new Error(
            `${command} failed: ${result.error?.message ?? result.stderr ?? result.status}`,
        );
    return result.stdout?.trim();
}
const bindingVersion = run("wasm-bindgen", ["--version"], true);
if (bindingVersion !== `wasm-bindgen ${version}`)
    throw new Error(
        `Install wasm-bindgen-cli ${version}: cargo install wasm-bindgen-cli --version ${version} --locked`,
    );
const metadata = JSON.parse(
    run(
        "cargo",
        [
            "metadata",
            "--manifest-path",
            manifest,
            "--filter-platform",
            "wasm32-unknown-unknown",
            "--format-version",
            "1",
            "--locked",
        ],
        true,
    ),
);
const bridge = metadata.packages.find(
    (packageInfo) => packageInfo.name === "ason-format-wasm",
);
const packageJson = JSON.parse(
    await readFile(resolve(root, "package.json"), "utf8"),
);
if (!bridge || bridge.version !== packageJson.version)
    throw new Error(
        "npm package version must match the ason-format-wasm crate version.",
    );
// Build the bridge and its dependencies for the WASM target, not the host target.
run("cargo", [
    "build",
    "--manifest-path",
    manifest,
    "-p",
    "ason-format-wasm",
    "--target",
    "wasm32-unknown-unknown",
    "--release",
    "--locked",
]);
const artifact = resolve(
// Hash Rust and manifest sources, excluding generated or explicitly ignored directories.
    metadata.target_directory,
    "wasm32-unknown-unknown/release/ason_format_wasm.wasm",
);
const output = resolve(root, "pkg");
await mkdir(output, { recursive: true });
run("wasm-bindgen", [
    artifact,
    "--target",
    "web",
    "--out-dir",
    output,
    "--out-name",
    "ason",
]);
const sourceHash = createHash("sha256");
async function hashSources(directory) {
    for (const entry of (
        await readdir(resolve(root, directory), { withFileTypes: true })
    ).sort((left, right) => left.name.localeCompare(right.name, "en"))) {
        if (entry.name === "IGNORE" || entry.name === "target") continue;
        const path = `${directory}/${entry.name}`;
        if (entry.isDirectory()) await hashSources(path);
        else if (entry.isFile() && /\.(rs|toml)$/.test(entry.name))
            sourceHash.update(path).update(await readFile(resolve(root, path)));
    }
}
for (const path of ["Cargo.toml", "Cargo.lock"])
    sourceHash.update(path).update(await readFile(resolve(root, path)));
await hashSources("crates");
const packages = new Map(
    metadata.packages.map((packageInfo) => [packageInfo.id, packageInfo]),
);
const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
const dependencies = new Set();
// Follow the resolved graph so the notices cover the bridge's full dependency tree.
function visit(id) {
    if (dependencies.has(id)) return;
    dependencies.add(id);
    for (const dependency of nodes.get(id)?.dependencies ?? [])
        visit(dependency);
}
visit(bridge.id);
const licenses = new Map();
const lines = [
    "# Bundled ASON WASM Dependency Notices",
    "",
    "Generated from the locked formatter source, including build dependencies.",
    "The generated WebAssembly bindings are covered by the wasm-bindgen notices below. This package has no npm runtime dependencies.",
    "",
];
for (const id of [...dependencies].sort()) {
    const packageInfo = packages.get(id);
    const directory = dirname(packageInfo.manifest_path);
    const entries = (await readdir(directory, { withFileTypes: true })).filter(
        (entry) =>
            /^(LICENSE|COPYING|NOTICE|UNLICENSE)([.\-_]|$)/i.test(entry.name),
    );
    const files = (
        await Promise.all(
            entries.map(async (entry) => {
                const file = resolve(directory, entry.name);
                return (await stat(file)).isFile() ? file : null;
            }),
        )
    ).filter(Boolean);
    if (packageInfo.license_file)
        files.push(resolve(directory, packageInfo.license_file));
    if (
        packageInfo.name === "hexfloat2" &&
        packageInfo.version === "0.2.0" &&
        packageInfo.license === "MIT" &&
        files.length === 0
    ) {
        // The pinned registry package omits its license text; bundle the reviewed local copy.
        files.push(resolve(root, "scripts/licenses/hexfloat2-0.2.0.txt"));
    }
    if (files.length === 0)
        throw new Error(
            `Missing license texts for ${packageInfo.name}; review before distributing.`,
        );
    lines.push(
        `- ${packageInfo.name} ${packageInfo.version}: ${packageInfo.license ?? "see license text"}`,
    );
    for (const file of new Set(files)) {
        const text = await readFile(file, "utf8");
        const digest = hash(text);
        const item = licenses.get(digest) ?? { names: new Set(), text };
        item.names.add(`${packageInfo.name} ${packageInfo.version}`);
        licenses.set(digest, item);
    }
}
for (const { names, text } of licenses.values())
    lines.push("", `## ${[...names].join(", ")}`, "", text);
const notices = lines.join("\n") + "\n";
await writeFile(resolve(root, "THIRD_PARTY_NOTICES.md"), notices);
const files = {};
for (const path of [
    "pkg/ason.js",
    "pkg/ason.d.ts",
    "pkg/ason_bg.wasm",
    "pkg/ason_bg.wasm.d.ts",
    "THIRD_PARTY_NOTICES.md",
    "LICENSE",
    "LICENSE.additional",
    "CONTRIBUTING",
]) {
    files[path] = hash(await readFile(resolve(root, path)));
}
// Record toolchain and content hashes to identify the generated package contents.
await writeFile(
    resolve(output, "build-info.json"),
    JSON.stringify(
        {
            formatterVersion: bridge.version,
            rust: run("rustc", ["--version"], true),
            wasmBindgen: bindingVersion,
            wasmSha256: files["pkg/ason_bg.wasm"],
            formatterSourceSha256: sourceHash.digest("hex"),
            cargoLockSha256: hash(await readFile(resolve(root, "Cargo.lock"))),
            noticesSha256: files["THIRD_PARTY_NOTICES.md"],
            files,
        },
        null,
        2,
    ) + "\n",
);
console.log(`Built ${packageJson.name}@${bridge.version}.`);
