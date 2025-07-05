import {
  beforeEach,
  afterEach,
  describe,
  expect,
  it,
  jest,
} from "@jest/globals";

let mockCore: any;
let mockExec: any;
let mockFs: any;
let mockSpawn: any;

const MOCK_CHILD_PROCESS = {
  pid: 12345,
  killed: false,
  exitCode: null,
  unref: jest.fn(),
};

beforeEach(() => {
  jest.resetModules();

  // Mock @actions/core
  mockCore = {
    debug: jest.fn(),
    error: jest.fn(),
    getInput: jest.fn(),
    info: jest.fn(),
    setFailed: jest.fn(),
    setOutput: jest.fn(),
    warning: jest.fn(),
    addPath: jest.fn(),
    saveState: jest.fn(),
    getState: jest.fn(),
  };

  // Mock @actions/exec
  mockExec = {
    exec: jest.fn().mockResolvedValue(0),
  };

  // Mock fs
  mockFs = {
    openSync: jest.fn().mockReturnValue(3),
    closeSync: jest.fn(),
    existsSync: jest.fn().mockReturnValue(true),
    readFileSync: jest.fn().mockReturnValue("test logs"),
  };

  // Mock child_process
  mockSpawn = jest.fn().mockReturnValue(MOCK_CHILD_PROCESS);

  // Mock process properties
  Object.defineProperty(process, "platform", {
    value: "linux",
    writable: true,
  });
  Object.defineProperty(process, "arch", { value: "x64", writable: true });
  process.getuid = jest.fn().mockReturnValue(1000);
  process.getgid = jest.fn().mockReturnValue(1000);
  jest.spyOn(process, "cwd").mockReturnValue("/test/workspace");

  // Setup module mocks
  jest.unstable_mockModule("@actions/core", () => mockCore);
  jest.unstable_mockModule("@actions/exec", () => mockExec);
  jest.unstable_mockModule("fs", () => mockFs);
  jest.unstable_mockModule("child_process", () => ({ spawn: mockSpawn }));
  jest.unstable_mockModule("path", () => ({
    join: jest.fn().mockReturnValue("/test/workspace/dist"),
  }));
});

afterEach(() => {
  jest.restoreAllMocks();
});

describe("Vorpal Setup Action", () => {
  describe("run function", () => {
    it("should complete successfully with default inputs", async () => {
      // Setup default inputs
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
          "registry-backend": "local",
          "registry-backend-s3-bucket": "",
          port: "23151",
          services: "agent,registry,worker",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      // Verify no errors were set
      expect(mockCore.setFailed).not.toHaveBeenCalled();
      expect(mockCore.info).toHaveBeenCalledWith("Installing Vorpal...");
      expect(mockCore.info).toHaveBeenCalledWith(
        "Setting up Vorpal directories...",
      );
      expect(mockCore.info).toHaveBeenCalledWith("Generating Vorpal keys...");
      expect(mockCore.info).toHaveBeenCalledWith("Starting Vorpal service...");
    });

    it("should handle local build correctly", async () => {
      mockCore.getInput.mockImplementation((name: string) => {
        if (name === "use-local-build") return "true";
        return "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockExec.exec).toHaveBeenCalledWith("chmod", [
        "+x",
        "./dist/vorpal",
      ]);
      expect(mockCore.addPath).toHaveBeenCalledWith("/test/workspace/dist");
    });

    it("should download and install binary when not using local build", async () => {
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockExec.exec).toHaveBeenCalledWith("curl", [
        "-sSL",
        "-o",
        "vorpal-x86_64-linux.tar.gz",
        "https://github.com/ALT-F4-LLC/vorpal/releases/download/v1.0.0/vorpal-x86_64-linux.tar.gz",
      ]);
      expect(mockExec.exec).toHaveBeenCalledWith("tar", [
        "-xzf",
        "vorpal-x86_64-linux.tar.gz",
      ]);
      expect(mockExec.exec).toHaveBeenCalledWith("chmod", ["+x", "vorpal"]);
    });

    it("should create all required directories", async () => {
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      const expectedDirectories = [
        "/var/lib/vorpal/key",
        "/var/lib/vorpal/sandbox",
        "/var/lib/vorpal/store",
        "/var/lib/vorpal/store/artifact/alias",
        "/var/lib/vorpal/store/artifact/archive",
        "/var/lib/vorpal/store/artifact/config",
        "/var/lib/vorpal/store/artifact/output",
      ];

      expectedDirectories.forEach((dir) => {
        expect(mockExec.exec).toHaveBeenCalledWith("sudo", [
          "mkdir",
          "-pv",
          dir,
        ]);
      });
    });

    it("should start vorpal with S3 backend", async () => {
      process.env.AWS_ACCESS_KEY_ID = "test-key";
      process.env.AWS_DEFAULT_REGION = "us-east-1";
      process.env.AWS_SECRET_ACCESS_KEY = "test-secret";

      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
          "registry-backend": "s3",
          "registry-backend-s3-bucket": "test-bucket",
          port: "23151",
          services: "agent,registry,worker",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockSpawn).toHaveBeenCalledWith(
        "vorpal",
        [
          "services",
          "start",
          "--port",
          "23151",
          "--services",
          "agent,registry,worker",
          "--registry-backend",
          "s3",
          "--registry-backend-s3-bucket",
          "test-bucket",
        ],
        expect.objectContaining({
          stdio: ["ignore", 3, 3],
          detached: true,
          env: expect.objectContaining({
            AWS_ACCESS_KEY_ID: "test-key",
            AWS_DEFAULT_REGION: "us-east-1",
            AWS_SECRET_ACCESS_KEY: "test-secret",
          }),
        }),
      );

      delete process.env.AWS_ACCESS_KEY_ID;
      delete process.env.AWS_DEFAULT_REGION;
      delete process.env.AWS_SECRET_ACCESS_KEY;
    });

    it("should handle errors and call setFailed", async () => {
      mockExec.exec.mockRejectedValueOnce(new Error("Command failed"));
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith("Command failed");
    });

    it("should throw error when S3 backend is missing bucket", async () => {
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
          "registry-backend": "s3",
          "registry-backend-s3-bucket": "",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith(
        "registry-backend-s3-bucket is required when using s3 backend",
      );
    });

    it("should throw error when AWS credentials are missing", async () => {
      delete process.env.AWS_ACCESS_KEY_ID;
      delete process.env.AWS_DEFAULT_REGION;
      delete process.env.AWS_SECRET_ACCESS_KEY;

      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
          "registry-backend": "s3",
          "registry-backend-s3-bucket": "test-bucket",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith(
        "AWS_ACCESS_KEY_ID environment variable is required when using s3 backend",
      );
    });

    it("should handle process startup failure", async () => {
      // Mock a failed process
      const failedChild = {
        pid: 12345,
        killed: true,
        exitCode: 1,
        unref: jest.fn(),
      };
      mockSpawn.mockReturnValue(failedChild);
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith(
        "Vorpal service failed to start",
      );
    });

    it("should handle darwin platform correctly", async () => {
      Object.defineProperty(process, "platform", { value: "darwin", writable: true });
      
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockExec.exec).toHaveBeenCalledWith("curl", [
        "-sSL",
        "-o",
        "vorpal-x86_64-darwin.tar.gz",
        "https://github.com/ALT-F4-LLC/vorpal/releases/download/v1.0.0/vorpal-x86_64-darwin.tar.gz"
      ]);
    });

    it("should handle arm64 architecture correctly", async () => {
      Object.defineProperty(process, "arch", { value: "arm64", writable: true });
      
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockExec.exec).toHaveBeenCalledWith("curl", [
        "-sSL",
        "-o",
        "vorpal-aarch64-linux.tar.gz",
        "https://github.com/ALT-F4-LLC/vorpal/releases/download/v1.0.0/vorpal-aarch64-linux.tar.gz"
      ]);
    });

    it("should throw error when AWS_DEFAULT_REGION is missing", async () => {
      process.env.AWS_ACCESS_KEY_ID = "test-key";
      delete process.env.AWS_DEFAULT_REGION;
      process.env.AWS_SECRET_ACCESS_KEY = "test-secret";

      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
          "registry-backend": "s3",
          "registry-backend-s3-bucket": "test-bucket",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith(
        "AWS_DEFAULT_REGION environment variable is required when using s3 backend"
      );

      delete process.env.AWS_ACCESS_KEY_ID;
      delete process.env.AWS_SECRET_ACCESS_KEY;
    });

    it("should throw error when AWS_SECRET_ACCESS_KEY is missing", async () => {
      process.env.AWS_ACCESS_KEY_ID = "test-key";
      process.env.AWS_DEFAULT_REGION = "us-east-1";
      delete process.env.AWS_SECRET_ACCESS_KEY;

      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
          "registry-backend": "s3",
          "registry-backend-s3-bucket": "test-bucket",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith(
        "AWS_SECRET_ACCESS_KEY environment variable is required when using s3 backend"
      );

      delete process.env.AWS_ACCESS_KEY_ID;
      delete process.env.AWS_DEFAULT_REGION;
    });

    it("should throw error when version is missing for remote build", async () => {
      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "", // Empty version
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith(
        "'version' input is required when 'use-local-build' is false."
      );
    });

    it("should handle non-Error exceptions in main catch block", async () => {
      mockCore.getInput.mockImplementation(() => {
        throw "String error"; // Throw non-Error exception
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith("String error");
    });

    it("should throw error when getuid/getgid are not supported", async () => {
      // Temporarily remove getuid and getgid to simulate Windows platform
      const originalGetuid = process.getuid;
      const originalGetgid = process.getgid;
      
      delete (process as any).getuid;
      delete (process as any).getgid;

      mockCore.getInput.mockImplementation((name: string) => {
        const inputs: Record<string, string> = {
          version: "v1.0.0",
          "use-local-build": "false",
        };
        return inputs[name] || "";
      });

      const { run } = await import("../src/index.js");
      await run();

      expect(mockCore.setFailed).toHaveBeenCalledWith(
        "Unable to get user/group ID - not supported on this platform"
      );

      // Restore original functions
      process.getuid = originalGetuid;
      process.getgid = originalGetgid;
    });
  });
});
