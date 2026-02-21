import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import * as grpc from "@grpc/grpc-js";
import type {
  Artifact as ArtifactMsg,
  ArtifactSource as ArtifactSourceMsg,
  ArtifactStep as ArtifactStepMsg,
  ArtifactStepSecret,
  ArtifactRequest,
  ArtifactsRequest,
  ArtifactsResponse,
} from "./api/artifact/artifact.js";
import {
  ArtifactSystem,
  ArtifactServiceClient,
} from "./api/artifact/artifact.js";
import {
  AgentServiceClient,
} from "./api/agent/agent.js";
import type {
  PrepareArtifactRequest,
  PrepareArtifactResponse,
} from "./api/agent/agent.js";
import {
  ContextServiceService,
} from "./api/context/context.js";
import { parseCliArgs } from "./cli.js";
import { getSystem } from "./system.js";

// ---------------------------------------------------------------------------
// TLS credential helper — matches Rust get_client_tls_config()
// ---------------------------------------------------------------------------

const VORPAL_ROOT_DIR = "/var/lib/vorpal";
const VORPAL_CA_PATH = join(VORPAL_ROOT_DIR, "key", "ca.pem");

function getClientCredentials(uri: string): grpc.ChannelCredentials {
  if (uri.startsWith("http://") || uri.startsWith("unix://")) {
    return grpc.credentials.createInsecure();
  }

  if (existsSync(VORPAL_CA_PATH)) {
    const caPem = readFileSync(VORPAL_CA_PATH);
    return grpc.credentials.createSsl(caPem);
  }

  // Use system roots (createSsl with no args uses Node's default CA store)
  return grpc.credentials.createSsl();
}

// ---------------------------------------------------------------------------
// Custom JSON serialization for cross-SDK parity
// ---------------------------------------------------------------------------

/**
 * Serializes an Artifact to JSON bytes matching Rust's serde_json::to_vec
 * output for prost-generated structs.
 *
 * Key differences from the generated toJSON:
 * - Field names are snake_case (matching proto field names)
 * - Field order follows proto field number order
 * - ALL fields are always included (even zero-values, empty arrays)
 * - Enums serialize as integers (not strings)
 * - Optional None serializes as null
 *
 * This matches what serde_json produces for prost structs with
 * #[derive(Serialize)] -- all fields present, in declaration order,
 * with no skip_serializing_if attributes.
 */
export function serializeArtifactStepSecret(secret: ArtifactStepSecret): object {
  return {
    name: secret.name,
    value: secret.value,
  };
}

export function serializeArtifactSource(source: ArtifactSourceMsg): object {
  return {
    digest: source.digest ?? null,
    excludes: source.excludes,
    includes: source.includes,
    name: source.name,
    path: source.path,
  };
}

export function serializeArtifactStep(step: ArtifactStepMsg): object {
  return {
    entrypoint: step.entrypoint ?? null,
    script: step.script ?? null,
    secrets: step.secrets.map(serializeArtifactStepSecret),
    arguments: step.arguments,
    artifacts: step.artifacts,
    environments: step.environments,
  };
}

export function serializeArtifact(artifact: ArtifactMsg): object {
  return {
    target: artifact.target,
    sources: artifact.sources.map(serializeArtifactSource),
    steps: artifact.steps.map(serializeArtifactStep),
    systems: artifact.systems,
    aliases: artifact.aliases,
    name: artifact.name,
  };
}

/**
 * Serializes an Artifact to a JSON string matching Rust serde_json::to_vec.
 * Returns the UTF-8 bytes of the JSON string.
 */
export function artifactToJsonBytes(artifact: ArtifactMsg): Buffer {
  const obj = serializeArtifact(artifact);
  const json = JSON.stringify(obj);
  return Buffer.from(json, "utf-8");
}

export function computeArtifactDigest(artifact: ArtifactMsg): string {
  const jsonBytes = artifactToJsonBytes(artifact);
  return createHash("sha256").update(jsonBytes).digest("hex");
}

// ---------------------------------------------------------------------------
// Artifact alias parsing
// ---------------------------------------------------------------------------

const DEFAULT_NAMESPACE = "library";
const DEFAULT_TAG = "latest";

export interface ArtifactAlias {
  name: string;
  namespace: string;
  tag: string;
}

/**
 * Formats an ArtifactAlias back into its canonical string representation.
 * Omits default namespace ("library") and default tag ("latest").
 */
export function formatArtifactAlias(alias: ArtifactAlias): string {
  const hasNamespace = alias.namespace !== DEFAULT_NAMESPACE;
  const hasTag = alias.tag !== DEFAULT_TAG;

  let result = "";
  if (hasNamespace) {
    result += `${alias.namespace}/`;
  }
  result += alias.name;
  if (hasTag) {
    result += `:${alias.tag}`;
  }
  return result;
}

function isValidComponent(s: string): boolean {
  if (s.length === 0) return false;
  for (const c of s) {
    if (
      !(
        (c >= "a" && c <= "z") ||
        (c >= "A" && c <= "Z") ||
        (c >= "0" && c <= "9") ||
        c === "-" ||
        c === "." ||
        c === "_" ||
        c === "+"
      )
    ) {
      return false;
    }
  }
  return true;
}

export function parseArtifactAlias(alias: string): ArtifactAlias {
  if (alias.length === 0) {
    throw new Error("alias cannot be empty");
  }

  if (alias.length > 255) {
    throw new Error("alias too long (max 255 characters)");
  }

  // Step 1: Extract tag (split on rightmost ':')
  let base: string;
  let tag: string;
  const lastColon = alias.lastIndexOf(":");
  if (lastColon !== -1) {
    const tagPart = alias.substring(lastColon + 1);
    if (tagPart === "") {
      throw new Error("tag cannot be empty");
    }
    tag = tagPart;
    base = alias.substring(0, lastColon);
  } else {
    tag = "";
    base = alias;
  }

  // Step 2: Extract namespace/name
  let namespace: string;
  let name: string;
  const slashIdx = base.indexOf("/");
  if (slashIdx === -1) {
    namespace = "";
    name = base;
  } else {
    namespace = base.substring(0, slashIdx);
    const rest = base.substring(slashIdx + 1);
    if (namespace === "") {
      throw new Error("namespace cannot be empty");
    }
    if (rest.includes("/")) {
      throw new Error("invalid format: too many path separators");
    }
    name = rest;
  }

  if (name === "") {
    throw new Error("name is required");
  }

  // Step 3: Validate component characters
  if (!isValidComponent(name)) {
    throw new Error(
      "name contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)",
    );
  }

  if (namespace !== "" && !isValidComponent(namespace)) {
    throw new Error(
      "namespace contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)",
    );
  }

  if (tag !== "" && !isValidComponent(tag)) {
    throw new Error(
      "tag contains invalid characters (allowed: alphanumeric, hyphens, dots, underscores, plus signs)",
    );
  }

  // Step 4: Apply defaults
  if (tag === "") {
    tag = DEFAULT_TAG;
  }

  if (namespace === "") {
    namespace = DEFAULT_NAMESPACE;
  }

  return { name, namespace, tag };
}

// ---------------------------------------------------------------------------
// ConfigContextStore
// ---------------------------------------------------------------------------

interface ConfigContextStore {
  artifact: Map<string, ArtifactMsg>;
  variable: Map<string, string>;
}

// ---------------------------------------------------------------------------
// ConfigContext
// ---------------------------------------------------------------------------

export class ConfigContext {
  private _artifact: string;
  private _artifactContext: string;
  private _artifactNamespace: string;
  private _artifactSystem: ArtifactSystem;
  private _artifactUnlock: boolean;
  private _clientAgent: AgentServiceClient;
  private _clientArtifact: ArtifactServiceClient;
  private _port: number;
  private _registry: string;
  private _store: ConfigContextStore;

  private constructor(
    artifact: string,
    artifactContext: string,
    artifactNamespace: string,
    artifactSystem: ArtifactSystem,
    artifactUnlock: boolean,
    clientAgent: AgentServiceClient,
    clientArtifact: ArtifactServiceClient,
    port: number,
    registry: string,
    store: ConfigContextStore,
  ) {
    this._artifact = artifact;
    this._artifactContext = artifactContext;
    this._artifactNamespace = artifactNamespace;
    this._artifactSystem = artifactSystem;
    this._artifactUnlock = artifactUnlock;
    this._clientAgent = clientAgent;
    this._clientArtifact = clientArtifact;
    this._port = port;
    this._registry = registry;
    this._store = store;
  }

  /**
   * Creates a ConfigContext by parsing CLI arguments and connecting to
   * gRPC services. Matches Rust get_context() and Go GetContext().
   */
  static create(argv?: string[]): ConfigContext {
    const args = parseCliArgs(argv);

    const artifactSystem = getSystem(args.artifactSystem);

    // Parse variables
    const variables = new Map<string, string>();
    for (const v of args.artifactVariable) {
      const eqIdx = v.indexOf("=");
      if (eqIdx !== -1) {
        const name = v.substring(0, eqIdx);
        const value = v.substring(eqIdx + 1);
        variables.set(name, value);
      }
    }

    // Create gRPC clients (TLS based on URI scheme, matching Rust SDK)
    const agentCredentials = getClientCredentials(args.agent);
    const registryCredentials = getClientCredentials(args.registry);
    const clientAgent = new AgentServiceClient(args.agent, agentCredentials);
    const clientArtifact = new ArtifactServiceClient(args.registry, registryCredentials);

    return new ConfigContext(
      args.artifact,
      args.artifactContext,
      args.artifactNamespace,
      artifactSystem,
      args.artifactUnlock,
      clientAgent,
      clientArtifact,
      args.port,
      args.registry,
      {
        artifact: new Map(),
        variable: variables,
      },
    );
  }

  /**
   * Adds an artifact to the context, computing its digest and sending it
   * to the agent service for preparation.
   *
   * The SHA-256 digest is computed from the JSON serialization of the
   * artifact, using the custom serializer that matches Rust's
   * serde_json::to_vec output.
   */
  async addArtifact(artifact: ArtifactMsg): Promise<string> {
    if (artifact.name === "") {
      throw new Error("name cannot be empty");
    }

    if (artifact.steps.length === 0) {
      throw new Error("steps cannot be empty");
    }

    if (artifact.systems.length === 0) {
      throw new Error("systems cannot be empty");
    }

    // Validate target is in systems list
    if (!artifact.systems.includes(artifact.target)) {
      throw new Error(
        `artifact '${artifact.name}' does not support system '${artifact.target}' (supported: ${artifact.systems.join(", ")})`,
      );
    }

    // Serialize and compute digest -- CRITICAL PATH for cross-SDK parity
    const artifactJson = artifactToJsonBytes(artifact);
    const artifactDigest = createHash("sha256")
      .update(artifactJson)
      .digest("hex");

    if (this._store.artifact.has(artifactDigest)) {
      return artifactDigest;
    }

    // Send to agent for preparation
    const request: PrepareArtifactRequest = {
      artifact_unlock: this._artifactUnlock,
      artifact_context: this._artifactContext,
      artifact_namespace: this._artifactNamespace,
      registry: this._registry,
      artifact: artifact,
    };

    const stream = this._clientAgent.prepareArtifact(request);

    let responseArtifact: ArtifactMsg | undefined;
    let responseArtifactDigest: string | undefined;

    await new Promise<void>((resolve, reject) => {
      stream.on("data", (response: PrepareArtifactResponse) => {
        if (response.artifact_output) {
          console.log(`${artifact.name} |> ${response.artifact_output}`);
        }

        if (response.artifact) {
          responseArtifact = response.artifact;
        }

        if (response.artifact_digest) {
          responseArtifactDigest = response.artifact_digest;
        }
      });

      stream.on("end", () => resolve());
      stream.on("error", (err: grpc.ServiceError) => {
        if (err.code === grpc.status.NOT_FOUND) {
          reject(new Error(`artifact '${artifact.name}' not found in agent service`));
        } else {
          reject(new Error(`gRPC error (code=${err.code}): ${err.message}`));
        }
      });
    });

    if (!responseArtifact) {
      throw new Error("artifact not returned from agent service");
    }

    if (!responseArtifactDigest) {
      throw new Error("artifact digest not returned from agent service");
    }

    this._store.artifact.set(responseArtifactDigest, responseArtifact);

    return responseArtifactDigest;
  }

  /**
   * Fetches an artifact by digest from the artifact service (registry).
   * Recursively fetches all step dependencies.
   */
  async fetchArtifact(digest: string): Promise<string> {
    if (this._store.artifact.has(digest)) {
      return digest;
    }

    const request: ArtifactRequest = {
      digest: digest,
      namespace: this._artifactNamespace,
    };

    const artifact = await new Promise<ArtifactMsg>((resolve, reject) => {
      this._clientArtifact.getArtifact(request, (err, response) => {
        if (err) {
          reject(new Error(`artifact service error: ${err.message}`));
        } else {
          resolve(response);
        }
      });
    });

    this._store.artifact.set(digest, artifact);

    for (const step of artifact.steps) {
      for (const dep of step.artifacts) {
        await this.fetchArtifact(dep);
      }
    }

    return digest;
  }

  /**
   * Fetches an artifact by alias from the artifact service (registry).
   * Uses the Go SDK approach: FetchArtifactAlias.
   */
  async fetchArtifactAlias(alias: string): Promise<string> {
    const parsed = parseArtifactAlias(alias);

    const request = {
      system: this._artifactSystem,
      name: parsed.name,
      namespace: parsed.namespace,
      tag: parsed.tag,
    };

    const response = await new Promise<{ digest: string }>((resolve, reject) => {
      this._clientArtifact.getArtifactAlias(request, (err, resp) => {
        if (err) {
          reject(new Error(`error fetching artifact alias: ${err.message}`));
        } else {
          resolve(resp);
        }
      });
    });

    const artifactDigest = response.digest;

    if (this._store.artifact.has(artifactDigest)) {
      return artifactDigest;
    }

    await this.fetchArtifact(artifactDigest);

    return artifactDigest;
  }

  getArtifactStore(): Map<string, ArtifactMsg> {
    return new Map(this._store.artifact);
  }

  getArtifact(digest: string): ArtifactMsg | undefined {
    return this._store.artifact.get(digest);
  }

  getArtifactContextPath(): string {
    return this._artifactContext;
  }

  getArtifactName(): string {
    return this._artifact;
  }

  getArtifactNamespace(): string {
    return this._artifactNamespace;
  }

  getSystem(): ArtifactSystem {
    return this._artifactSystem;
  }

  getVariable(name: string): string | undefined {
    return this._store.variable.get(name);
  }

  /**
   * Starts the ContextService gRPC server.
   * Matches Rust ConfigContext::run() and Go ConfigContext.Run().
   *
   * Prints "context service: [::]:PORT" to stdout for CLI detection.
   */
  async run(): Promise<void> {
    const server = new grpc.Server();

    const store = this._store;

    server.addService(ContextServiceService, {
      getArtifact: (
        call: grpc.ServerUnaryCall<ArtifactRequest, ArtifactMsg>,
        callback: grpc.sendUnaryData<ArtifactMsg>,
      ) => {
        const request = call.request;

        if (!request.digest || request.digest === "") {
          callback({
            code: grpc.status.INVALID_ARGUMENT,
            message: "'digest' is required",
          });
          return;
        }

        const artifact = store.artifact.get(request.digest);

        if (!artifact) {
          callback({
            code: grpc.status.NOT_FOUND,
            message: "artifact not found",
          });
          return;
        }

        callback(null, artifact);
      },

      getArtifacts: (
        _call: grpc.ServerUnaryCall<ArtifactsRequest, ArtifactsResponse>,
        callback: grpc.sendUnaryData<ArtifactsResponse>,
      ) => {
        const digests = Array.from(store.artifact.keys()).sort();
        callback(null, { digests });
      },
    });

    const addr = `[::]:${this._port}`;

    await new Promise<void>((resolve, reject) => {
      server.bindAsync(addr, grpc.ServerCredentials.createInsecure(), (err) => {
        if (err) {
          reject(new Error(`failed to bind: ${err.message}`));
          return;
        }
        resolve();
      });
    });

    console.log(`context service: ${addr}`);

    // Keep the server running until SIGINT/SIGTERM
    await new Promise<void>((resolve) => {
      const shutdown = () => {
        server.tryShutdown((err) => {
          if (err) {
            console.error(`shutdown error: ${err.message}`);
            server.forceShutdown();
          }
          resolve();
        });
      };

      process.on("SIGINT", shutdown);
      process.on("SIGTERM", shutdown);
    });
  }
}
