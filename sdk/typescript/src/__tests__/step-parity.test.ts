import { describe, expect, test } from "bun:test";
import { bash, bwrap, docker } from "../artifact/step.js";
import { getEnvKey } from "../artifact.js";
import type { ArtifactStep, ArtifactStepSecret } from "../api/artifact/artifact.js";

describe("step parity", () => {
  // -----------------------------------------------------------------
  // bash() step
  // -----------------------------------------------------------------

  describe("bash()", () => {
    test("basic bash step with no artifacts or environments", () => {
      const step = bash([], [], [], "echo hello");

      expect(step.entrypoint).toBe("bash");
      expect(step.script).toBe("#!/bin/bash\nset -euo pipefail\n\necho hello\n");
      expect(step.arguments).toEqual([]);
      expect(step.artifacts).toEqual([]);
      expect(step.environments).toEqual([
        "HOME=$VORPAL_WORKSPACE",
        "PATH=:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
      ]);
    });

    test("bash step PATH construction with artifacts", () => {
      const artifacts = ["abc123", "def456"];
      const step = bash(artifacts, [], [], "echo test");

      const expectedPath = `${getEnvKey("abc123")}/bin:${getEnvKey("def456")}/bin:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin`;
      expect(step.environments).toContain(`PATH=${expectedPath}`);
      expect(step.environments).toContain("HOME=$VORPAL_WORKSPACE");
    });

    test("bash step filters PATH from environments", () => {
      const environments = ["CUSTOM=value", "PATH=/extra/bin", "FOO=bar"];
      const step = bash([], environments, [], "echo test");

      // PATH= environments should be filtered, then path prepended
      expect(step.environments).not.toContain("PATH=/extra/bin");
      expect(step.environments).toContain("CUSTOM=value");
      expect(step.environments).toContain("FOO=bar");
      // PATH should be: /extra/bin:DEFAULT
      const pathEnv = step.environments.find((e) => e.startsWith("PATH="));
      expect(pathEnv).toBe(
        "PATH=/extra/bin::/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
      );
    });

    test("bash step PATH construction with artifacts and user PATH", () => {
      const artifacts = ["abc123"];
      const environments = ["PATH=/custom/path"];
      const step = bash(artifacts, environments, [], "echo test");

      const expectedPath = `/custom/path:${getEnvKey("abc123")}/bin:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin`;
      const pathEnv = step.environments.find((e) => e.startsWith("PATH="));
      expect(pathEnv).toBe(`PATH=${expectedPath}`);
    });

    test("bash step script wrapping", () => {
      const script = "make build\nmake test";
      const step = bash([], [], [], script);

      expect(step.script).toBe(
        "#!/bin/bash\nset -euo pipefail\n\nmake build\nmake test\n",
      );
    });

    test("bash step deduplicates secrets by name", () => {
      const secrets: ArtifactStepSecret[] = [
        { name: "KEY", value: "first" },
        { name: "KEY", value: "second" },
        { name: "OTHER", value: "val" },
      ];
      const step = bash([], [], secrets, "echo test");

      expect(step.secrets).toHaveLength(2);
      expect(step.secrets[0]).toEqual({ name: "KEY", value: "first" });
      expect(step.secrets[1]).toEqual({ name: "OTHER", value: "val" });
    });

    test("bash step HOME and PATH are last in environments", () => {
      const step = bash([], ["CUSTOM=val"], [], "echo test");
      const envs = step.environments;

      expect(envs[envs.length - 2]).toBe("HOME=$VORPAL_WORKSPACE");
      expect(envs[envs.length - 1]).toMatch(/^PATH=/);
    });
  });

  // -----------------------------------------------------------------
  // bwrap() step
  // -----------------------------------------------------------------

  describe("bwrap()", () => {
    test("basic bwrap step with no rootfs", () => {
      const step = bwrap([], [], [], null, [], "echo test");

      expect(step.entrypoint).toBe("bwrap");
      expect(step.script).toBe("#!/bin/bash\nset -euo pipefail\n\necho test\n");
      expect(step.environments).toEqual([
        "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
      ]);
    });

    test("bwrap base arguments ordering", () => {
      const step = bwrap([], [], [], null, [], "echo test");

      const expectedBaseArgs = [
        "--unshare-all",
        "--share-net",
        "--clearenv",
        "--chdir",
        "$VORPAL_WORKSPACE",
        "--gid",
        "1000",
        "--uid",
        "1000",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--bind",
        "$VORPAL_OUTPUT",
        "$VORPAL_OUTPUT",
        "--bind",
        "$VORPAL_WORKSPACE",
        "$VORPAL_WORKSPACE",
        "--setenv",
        "VORPAL_OUTPUT",
        "$VORPAL_OUTPUT",
        "--setenv",
        "VORPAL_WORKSPACE",
        "$VORPAL_WORKSPACE",
        "--setenv",
        "HOME",
        "$VORPAL_WORKSPACE",
      ];

      // Base args should be the prefix of the full args list
      for (let i = 0; i < expectedBaseArgs.length; i++) {
        expect(step.arguments[i]).toBe(expectedBaseArgs[i]);
      }
    });

    test("bwrap rootfs bind mounts", () => {
      const rootfs = "rootfs-digest";
      const step = bwrap([], [], [], rootfs, [], "echo test");

      const rootfsEnv = getEnvKey(rootfs);
      const args = step.arguments;

      // Check rootfs bind mounts appear after base args
      expect(args).toContain("--ro-bind");
      expect(args).toContain(`${rootfsEnv}/bin`);
      expect(args).toContain("/bin");
      expect(args).toContain(`${rootfsEnv}/etc`);
      expect(args).toContain("/etc");
      expect(args).toContain(`${rootfsEnv}/lib`);
      expect(args).toContain("/lib");
      expect(args).toContain("--ro-bind-try");
      expect(args).toContain(`${rootfsEnv}/lib64`);
      expect(args).toContain("/lib64");
      expect(args).toContain(`${rootfsEnv}/sbin`);
      expect(args).toContain("/sbin");
      expect(args).toContain(`${rootfsEnv}/usr`);
      expect(args).toContain("/usr");

      // rootfs should be in artifacts
      expect(step.artifacts).toContain(rootfs);
    });

    test("bwrap artifact bind mounts and setenv", () => {
      const artifacts = ["dep1", "dep2"];
      const step = bwrap([], artifacts, [], null, [], "echo test");

      for (const artifact of artifacts) {
        const envKey = getEnvKey(artifact);
        expect(step.arguments).toContain(envKey);
        // setenv for each artifact
        expect(step.arguments).toContain(envKey.replace("$", ""));
      }

      expect(step.artifacts).toEqual(artifacts);
    });

    test("bwrap rootfs plus additional artifacts", () => {
      const rootfs = "rootfs-digest";
      const artifacts = ["extra-dep"];
      const step = bwrap([], artifacts, [], rootfs, [], "echo test");

      // step.artifacts should be [rootfs, ...artifacts]
      expect(step.artifacts[0]).toBe(rootfs);
      expect(step.artifacts[1]).toBe("extra-dep");
    });

    test("bwrap PATH includes all artifact bins", () => {
      const artifacts = ["dep1", "dep2"];
      const step = bwrap([], artifacts, [], null, [], "echo test");

      const pathArg = step.arguments[step.arguments.indexOf("PATH") + 1];
      for (const artifact of artifacts) {
        expect(pathArg).toContain(`${getEnvKey(artifact)}/bin`);
      }
    });

    test("bwrap environment arguments", () => {
      const environments = ["CUSTOM=value", "FOO=bar"];
      const step = bwrap([], [], environments, null, [], "echo test");

      const args = step.arguments;
      // Non-PATH environments should be added via --setenv
      const customIdx = args.indexOf("CUSTOM");
      expect(customIdx).toBeGreaterThan(-1);
      expect(args[customIdx - 1]).toBe("--setenv");
      expect(args[customIdx + 1]).toBe("value");
    });

    test("bwrap filters PATH from environment arguments", () => {
      const environments = ["PATH=/extra", "CUSTOM=val"];
      const step = bwrap([], [], environments, null, [], "echo test");

      const args = step.arguments;
      // PATH should be handled specially, not added as --setenv PATH /extra
      // But CUSTOM should still be there
      const customIdx = args.indexOf("CUSTOM");
      expect(customIdx).toBeGreaterThan(-1);
    });

    test("bwrap custom arguments appended at end", () => {
      const customArgs = ["--custom-flag", "custom-value"];
      const step = bwrap(customArgs, [], [], null, [], "echo test");

      const args = step.arguments;
      expect(args[args.length - 2]).toBe("--custom-flag");
      expect(args[args.length - 1]).toBe("custom-value");
    });

    test("bwrap deduplicates secrets", () => {
      const secrets: ArtifactStepSecret[] = [
        { name: "KEY", value: "first" },
        { name: "KEY", value: "second" },
      ];
      const step = bwrap([], [], [], null, secrets, "echo test");

      expect(step.secrets).toHaveLength(1);
      expect(step.secrets[0]).toEqual({ name: "KEY", value: "first" });
    });
  });

  // -----------------------------------------------------------------
  // docker() step
  // -----------------------------------------------------------------

  describe("docker()", () => {
    test("basic docker step", () => {
      const args = ["build", "-t", "myimage"];
      const artifacts = ["dep1"];
      const step = docker(args, artifacts);

      expect(step.entrypoint).toBe("docker");
      expect(step.script).toBeUndefined();
      expect(step.secrets).toEqual([]);
      expect(step.arguments).toEqual(args);
      expect(step.artifacts).toEqual(artifacts);
      expect(step.environments).toEqual([
        "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
      ]);
    });

    test("docker step with empty arguments and artifacts", () => {
      const step = docker([], []);

      expect(step.entrypoint).toBe("docker");
      expect(step.arguments).toEqual([]);
      expect(step.artifacts).toEqual([]);
    });
  });
});
