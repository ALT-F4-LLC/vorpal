import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// ---------------------------------------------------------------------------
// Template paths (relative to the repository root)
// ---------------------------------------------------------------------------

// Resolve repo root from process.cwd() (bun test runs from sdk/typescript)
const REPO_ROOT = join(process.cwd(), "../..");
const TEMPLATE_DIR = join(
  REPO_ROOT,
  "cli/src/command/template/typescript",
);

// ---------------------------------------------------------------------------
// Template generation — all expected files exist and have content
// ---------------------------------------------------------------------------

describe("template generation", () => {
  const EXPECTED_FILES = [
    "Vorpal.toml",
    "src/vorpal.ts",
    "src/main.ts",
    "package.json",
    "tsconfig.json",
  ];

  for (const file of EXPECTED_FILES) {
    test(`template file exists: ${file}`, () => {
      const filePath = join(TEMPLATE_DIR, file);
      const content = readFileSync(filePath, "utf-8");
      expect(content.length).toBeGreaterThan(0);
    });
  }

  test("Vorpal.toml has language = typescript", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "Vorpal.toml"),
      "utf-8",
    );
    expect(content).toContain('language = "typescript"');
  });

  test("Vorpal.toml has name = example", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "Vorpal.toml"),
      "utf-8",
    );
    expect(content).toContain('name = "example"');
  });

  test("Vorpal.toml includes required source files", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "Vorpal.toml"),
      "utf-8",
    );
    expect(content).toContain("[source]");
    expect(content).toContain("includes");
    expect(content).toContain("package.json");
    expect(content).toContain("tsconfig.json");
    expect(content).toContain("bun.lockb");
  });

  test("package.json has correct structure", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "package.json"),
      "utf-8",
    );
    const pkg = JSON.parse(content);
    expect(pkg.name).toBe("example");
    expect(pkg.type).toBe("module");
    expect(pkg.dependencies).toBeDefined();
    expect(pkg.dependencies["@vorpal/sdk"]).toBeDefined();
    expect(pkg.devDependencies).toBeDefined();
    expect(pkg.devDependencies["typescript"]).toBeDefined();
    expect(pkg.devDependencies["@types/bun"]).toBeDefined();
  });

  test("package.json has build and start scripts", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "package.json"),
      "utf-8",
    );
    const pkg = JSON.parse(content);
    expect(pkg.scripts).toBeDefined();
    expect(pkg.scripts.build).toBeDefined();
    expect(pkg.scripts.start).toBeDefined();
  });

  test("tsconfig.json has valid compiler options", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "tsconfig.json"),
      "utf-8",
    );
    const tsconfig = JSON.parse(content);
    expect(tsconfig.compilerOptions).toBeDefined();
    expect(tsconfig.compilerOptions.strict).toBe(true);
    expect(tsconfig.compilerOptions.target).toBeDefined();
    expect(tsconfig.compilerOptions.module).toBeDefined();
    expect(tsconfig.compilerOptions.outDir).toBeDefined();
    expect(tsconfig.compilerOptions.rootDir).toBe("./src");
    expect(tsconfig.include).toContain("src");
  });

  test("tsconfig.json does not require explicit bun-types (provided by @types/bun)", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "tsconfig.json"),
      "utf-8",
    );
    const tsconfig = JSON.parse(content);
    expect(tsconfig.compilerOptions.types).toBeUndefined();
  });

  test("src/vorpal.ts imports from @vorpal/sdk", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "src/vorpal.ts"),
      "utf-8",
    );
    expect(content).toContain("@vorpal/sdk");
  });

  test("src/vorpal.ts uses ConfigContext.create()", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "src/vorpal.ts"),
      "utf-8",
    );
    expect(content).toContain("ConfigContext.create()");
  });

  test("src/vorpal.ts uses context.run()", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "src/vorpal.ts"),
      "utf-8",
    );
    expect(content).toContain("context.run()");
  });

  test("src/vorpal.ts defines all four systems", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "src/vorpal.ts"),
      "utf-8",
    );
    expect(content).toContain("AARCH64_DARWIN");
    expect(content).toContain("AARCH64_LINUX");
    expect(content).toContain("X8664_DARWIN");
    expect(content).toContain("X8664_LINUX");
  });

  test("src/vorpal.ts uses JobBuilder", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "src/vorpal.ts"),
      "utf-8",
    );
    expect(content).toContain("JobBuilder");
  });

  test("src/main.ts contains valid program entry", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "src/main.ts"),
      "utf-8",
    );
    expect(content).toContain("console.log");
  });
});

// ---------------------------------------------------------------------------
// Template syntax validation
// ---------------------------------------------------------------------------

describe("template syntax validation", () => {
  test("src/vorpal.ts has valid TypeScript syntax (parsed by Bun)", async () => {
    const filePath = join(TEMPLATE_DIR, "src/vorpal.ts");
    const transpiler = new Bun.Transpiler({ loader: "ts" });
    const source = readFileSync(filePath, "utf-8");
    const result = transpiler.transformSync(source);
    expect(result.length).toBeGreaterThan(0);
  });

  test("src/main.ts has valid TypeScript syntax (parsed by Bun)", () => {
    const filePath = join(TEMPLATE_DIR, "src/main.ts");
    const transpiler = new Bun.Transpiler({ loader: "ts" });
    const source = readFileSync(filePath, "utf-8");
    const result = transpiler.transformSync(source);
    expect(result.length).toBeGreaterThan(0);
  });

  test("package.json is valid JSON", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "package.json"),
      "utf-8",
    );
    expect(() => JSON.parse(content)).not.toThrow();
  });

  test("tsconfig.json is valid JSON", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "tsconfig.json"),
      "utf-8",
    );
    expect(() => JSON.parse(content)).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// Template name substitution
//
// The init.rs code replaces "example" with the project name in both file
// paths and file content. Verify the template uses "example" consistently
// so that substitution works.
// ---------------------------------------------------------------------------

describe("template name substitution", () => {
  test("Vorpal.toml uses 'example' as the name", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "Vorpal.toml"),
      "utf-8",
    );
    expect(content).toContain('name = "example"');
  });

  test("package.json uses 'example' as the name", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "package.json"),
      "utf-8",
    );
    const pkg = JSON.parse(content);
    expect(pkg.name).toBe("example");
  });

  test("src/vorpal.ts uses 'example' in the JobBuilder name", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "src/vorpal.ts"),
      "utf-8",
    );
    expect(content).toContain('"example"');
  });
});

// ---------------------------------------------------------------------------
// Vorpal.toml config parsing
// ---------------------------------------------------------------------------

describe("Vorpal.toml config parsing", () => {
  test("TypeScript Vorpal.toml parses as valid TOML", () => {
    const content = readFileSync(
      join(TEMPLATE_DIR, "Vorpal.toml"),
      "utf-8",
    );
    const lines = content.split("\n").map((l) => l.trim());

    const langLine = lines.find((l) =>
      l.startsWith("language"),
    );
    expect(langLine).toBeDefined();
    expect(langLine).toContain('"typescript"');

    const nameLine = lines.find((l) => l.startsWith("name"));
    expect(nameLine).toBeDefined();

    expect(lines.some((l) => l === "[source]")).toBe(true);
  });

  test("TypeScript template differs from Go and Rust templates", () => {
    const tsContent = readFileSync(
      join(TEMPLATE_DIR, "Vorpal.toml"),
      "utf-8",
    );
    const goContent = readFileSync(
      join(REPO_ROOT, "cli/src/command/template/go/Vorpal.toml"),
      "utf-8",
    );
    const rustContent = readFileSync(
      join(REPO_ROOT, "cli/src/command/template/rust/Vorpal.toml"),
      "utf-8",
    );

    expect(tsContent).toContain('"typescript"');
    expect(goContent).toContain('"go"');
    expect(rustContent).toContain('"rust"');

    expect(tsContent).not.toBe(goContent);
    expect(tsContent).not.toBe(rustContent);
  });
});
