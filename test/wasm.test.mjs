import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import init, {
    formatDocument,
    validateDocument,
} from "@hemashushu/ason-format-wasm";

await init({
    module_or_path: await readFile(
        new URL(
            import.meta.resolve("@hemashushu/ason-format-wasm/ason_bg.wasm"),
        ),
    ),
});
const format = (source, spaces = true, crlf = false) =>
    JSON.parse(formatDocument(source, 2, spaces, crlf));

test("WASM keeps literals/comments intact with all structural layouts", () => {
    const source = '{a:"one\n two", /* c\r\n d */ b:r##"raw\r\ntext"##}';
    for (const spaces of [true, false])
        for (const crlf of [true, false]) {
            const result = format(source, spaces, crlf);
            assert.equal(result.status, "success");
            for (const literal of [
                '"one\n two"',
                "/* c\r\n d */",
                'r##"raw\r\ntext"##',
            ])
                assert.ok(result.text.includes(literal));
            assert.equal(format(result.text, spaces, crlf).text, result.text);
            assert.ok(
                result.text.includes(
                    `${crlf ? "\r\n" : "\n"}${spaces ? "  " : "\t"}a:`,
                ),
            );
        }
});

test("WASM structured diagnostics use UTF-16 exclusive ranges", () => {
    const source = "{\u{1f600}:1\r\n\u{1f600}:2}";
    const result = JSON.parse(validateDocument(source));
    assert.equal(result.status, "error");
    assert.equal(
        source.slice(result.error.start, result.error.end),
        "\u{1f600}",
    );
    assert.equal(result.error.start, "{\u{1f600}:1\r\n".length);
    const typeError = JSON.parse(validateDocument("[1 true]")).error;
    assert.deepEqual(
        [typeError.kind, typeError.start, typeError.end],
        ["syntax", 3, 7],
    );
    const reserved = "{\u{1f600}: Some::A}";
    const reservedError = JSON.parse(validateDocument(reserved)).error;
    assert.equal(
        reserved.slice(reservedError.start, reservedError.end),
        "Some",
    );
    assert.equal(JSON.parse(validateDocument("[")).error.start, 1);
    for (const value of [
        'd"2024-03-16T16:30:50+08:00"',
        "0x1.8p+2_f64",
        'Option::Some("\u4e2d")',
    ])
        assert.equal(JSON.parse(validateDocument(value)).status, "success");
});

test("WASM validates without formatting and never partially formats invalid input", () => {
    assert.deepEqual(JSON.parse(validateDocument("{answer:42}")), {
        status: "success",
        text: null,
    });
    assert.deepEqual(format("{answer:42}"), {
        status: "success",
        text: "{answer: 42}\n",
    });
    for (const source of [
        "",
        "// only a comment",
        "{a:1 a:2}",
        "[1 true]",
        "[",
    ]) {
        const result = format(source);
        assert.equal(result.status, "error");
        assert.equal(typeof result.error.message, "string");
        assert.equal(Object.hasOwn(result, "text"), false);
    }
});

test("npm exports include matching WASM, bindings, types and license notices", async () => {
    const root = new URL("../", import.meta.url);
    const packageJson = JSON.parse(
        await readFile(new URL("package.json", root), "utf8"),
    );
    const info = JSON.parse(
        await readFile(
            new URL(
                import.meta
                    .resolve("@hemashushu/ason-format-wasm/build-info.json"),
            ),
            "utf8",
        ),
    );
    assert.equal(info.formatterVersion, packageJson.version);
    assert.equal(info.wasmBindgen, "wasm-bindgen 0.2.130");
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
        const digest = createHash("sha256")
            .update(await readFile(new URL(path, root)))
            .digest("hex");
        assert.equal(info.files[path], digest, path);
    }
    assert.equal(info.wasmSha256, info.files["pkg/ason_bg.wasm"]);
    assert.equal(info.noticesSha256, info.files["THIRD_PARTY_NOTICES.md"]);
    const notices = await readFile(
        new URL(
            import.meta
                .resolve("@hemashushu/ason-format-wasm/THIRD_PARTY_NOTICES.md"),
        ),
        "utf8",
    );
    assert.ok(notices.includes("hexfloat2 0.2.0"));
    assert.ok(notices.includes("wasm-bindgen 0.2.130"));
});

test(
    "packed npm package works outside the formatter checkout without build scripts",
    { timeout: 60000 },
    async () => {
        const temporary = await mkdtemp(join(tmpdir(), "ason-format-wasm-"));
        const root = fileURLToPath(new URL("../", import.meta.url));
        function npm(args, cwd) {
            const result = process.env.npm_execpath
                ? spawnSync(
                      process.execPath,
                      [process.env.npm_execpath, ...args],
                      { cwd, encoding: "utf8" },
                  )
                : spawnSync("npm", args, {
                      cwd,
                      encoding: "utf8",
                      shell: process.platform === "win32",
                  });
            assert.equal(
                result.status,
                0,
                result.error?.message ?? result.stderr,
            );
            return result.stdout;
        }
        try {
            const [packed] = JSON.parse(
                npm(
                    [
                        "pack",
                        "--ignore-scripts",
                        "--json",
                        "--pack-destination",
                        temporary,
                    ],
                    root,
                ),
            );
            assert.deepEqual(
                packed.files.map((file) => file.path).sort(),
                [
                    "CONTRIBUTING",
                    "LICENSE",
                    "LICENSE.additional",
                    "README.md",
                    "THIRD_PARTY_NOTICES.md",
                    "package.json",
                    "pkg/ason.d.ts",
                    "pkg/ason.js",
                    "pkg/ason_bg.wasm",
                    "pkg/ason_bg.wasm.d.ts",
                    "pkg/build-info.json",
                ].sort(),
            );
            npm(
                [
                    "install",
                    "--ignore-scripts",
                    "--no-audit",
                    "--no-fund",
                    join(temporary, packed.filename),
                ],
                temporary,
            );
            const result = spawnSync(
                process.execPath,
                [
                    "--input-type=module",
                    "--eval",
                    `
      import assert from 'node:assert/strict';
      import { readFile } from 'node:fs/promises';
      import { createHash } from 'node:crypto';
      import init, { formatDocument, validateDocument } from '@hemashushu/ason-format-wasm';
      const root = new URL('./', import.meta.resolve('@hemashushu/ason-format-wasm/package.json'));
      const info = JSON.parse(await readFile(new URL(import.meta.resolve('@hemashushu/ason-format-wasm/build-info.json')), 'utf8'));
      for (const [path, expected] of Object.entries(info.files)) {
        assert.equal(createHash('sha256').update(await readFile(new URL(path, root))).digest('hex'), expected, path);
      }
      await init({ module_or_path: await readFile(new URL(import.meta.resolve('@hemashushu/ason-format-wasm/ason_bg.wasm'))) });
      assert.deepEqual(JSON.parse(formatDocument('{answer:42}', 2, true, false)), { status: 'success', text: '{answer: 42}\\n' });
      assert.deepEqual(JSON.parse(validateDocument('{answer:42}')), { status: 'success', text: null });
    `,
                ],
                { cwd: temporary, encoding: "utf8" },
            );
            assert.equal(
                result.status,
                0,
                result.error?.message ?? result.stderr,
            );
        } finally {
            await rm(temporary, { recursive: true, force: true });
        }
    },
);
