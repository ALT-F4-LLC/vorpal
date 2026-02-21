/**
 * CLI argument parsing for the Vorpal SDK "start" subcommand.
 *
 * Matches the Rust SDK's cli.rs (clap-based) argument structure:
 *   start --agent URL --artifact NAME --artifact-context PATH
 *         --artifact-namespace NS --artifact-system SYS
 *         --port PORT --registry URL
 *         [--artifact-unlock] [--artifact-variable KEY=VALUE...]
 */

export interface StartCommand {
  agent: string;
  artifact: string;
  artifactContext: string;
  artifactNamespace: string;
  artifactSystem: string;
  artifactUnlock: boolean;
  artifactVariable: string[];
  port: number;
  registry: string;
}

function getDefaultAddress(): string {
  const envSocket = process.env["VORPAL_SOCKET_PATH"];
  if (envSocket && envSocket.trim() !== "") {
    return `unix://${envSocket}`;
  }
  return "unix:///var/lib/vorpal/vorpal.sock";
}

/**
 * Parses CLI arguments matching the Rust SDK's clap structure.
 * Expects: start --flag value [--flag value ...]
 */
export function parseCliArgs(argv: string[] = process.argv.slice(2)): StartCommand {
  if (argv.length === 0 || argv[0] !== "start") {
    throw new Error("expected 'start' subcommand");
  }

  const args = argv.slice(1);
  const defaultAddress = getDefaultAddress();

  let agent: string = defaultAddress;
  let artifact: string = "";
  let artifactContext: string = "";
  let artifactNamespace: string = "";
  let artifactSystem: string = "";
  let artifactUnlock: boolean = false;
  const artifactVariable: string[] = [];
  let port: number = 0;
  let portProvided: boolean = false;
  let registry: string = defaultAddress;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];

    function consumeValue(flag: string): string {
      i++;
      if (i >= args.length) {
        throw new Error(`${flag} requires a value`);
      }
      return args[i];
    }

    switch (arg) {
      case "--agent":
        agent = consumeValue(arg);
        break;
      case "--artifact":
        artifact = consumeValue(arg);
        break;
      case "--artifact-context":
        artifactContext = consumeValue(arg);
        break;
      case "--artifact-namespace":
        artifactNamespace = consumeValue(arg);
        break;
      case "--artifact-system":
        artifactSystem = consumeValue(arg);
        break;
      case "--artifact-unlock":
        artifactUnlock = true;
        break;
      case "--artifact-variable":
        artifactVariable.push(consumeValue(arg));
        break;
      case "--port": {
        const portStr = consumeValue(arg);
        const parsed = parseInt(portStr, 10);
        if (isNaN(parsed)) {
          throw new Error(`--port value is not a valid number: ${portStr}`);
        }
        port = parsed;
        portProvided = true;
        break;
      }
      case "--registry":
        registry = consumeValue(arg);
        break;
      default:
        throw new Error(`unknown argument: ${arg}`);
    }
  }

  if (!artifact) {
    throw new Error("--artifact is required");
  }

  if (!artifactContext) {
    throw new Error("--artifact-context is required");
  }

  if (!artifactNamespace) {
    throw new Error("--artifact-namespace is required");
  }

  if (!artifactSystem) {
    throw new Error("--artifact-system is required");
  }

  if (!portProvided) {
    throw new Error("--port is required");
  }

  return {
    agent,
    artifact,
    artifactContext,
    artifactNamespace,
    artifactSystem,
    artifactUnlock,
    artifactVariable,
    port,
    registry,
  };
}
