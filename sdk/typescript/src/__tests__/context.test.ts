import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  type Artifact as ArtifactMsg,
  type ArtifactStepSecret,
  ArtifactSystem,
} from "../api/artifact/artifact.js";
import { ArtifactStep, secretsToProto } from "../artifact.js";
import {
  artifactToJsonBytes,
  clientAuthHeader,
  computeArtifactDigest,
  serializeArtifactStepSecret,
} from "../context.js";

const ISSUER = "https://issuer.example";
const TOKEN_ENDPOINT = "https://issuer.example/oauth/token";
const REGISTRY = "https://registry.example";

interface CredentialsFixtureOptions {
  /** Seconds since the epoch to record as `issued_at`. Defaults to a long-expired token. */
  issuedAt?: number;
  /** Lifetime of the access token in seconds. Defaults to 3600. */
  expiresIn?: number;
  /** Refresh token already on disk before the call. */
  refreshToken?: string;
}

function writeCredentialsFile(path: string, opts: CredentialsFixtureOptions = {}): void {
  const issuedAt = opts.issuedAt ?? 0;
  const expiresIn = opts.expiresIn ?? 3600;
  const refreshToken = opts.refreshToken ?? "old-refresh-token";

  const credentials = {
    issuer: {
      [ISSUER]: {
        access_token: "old-access-token",
        audience: "vorpal",
        client_id: "vorpal-cli",
        expires_in: expiresIn,
        issued_at: issuedAt,
        refresh_token: refreshToken,
        scopes: ["openid", "offline_access"],
      },
    },
    registry: {
      [REGISTRY]: ISSUER,
    },
  };

  writeFileSync(path, JSON.stringify(credentials, null, 2), { mode: 0o600 });
}

interface FetchScenario {
  /** Body returned by the token endpoint. */
  tokenResponse: Record<string, unknown>;
}

function installFetchMock(scenario: FetchScenario): ReturnType<typeof mock> {
  const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
    const url = typeof input === "string" ? input : input.toString();
    if (url === `${ISSUER}/.well-known/openid-configuration`) {
      return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (url === TOKEN_ENDPOINT) {
      return new Response(JSON.stringify(scenario.tokenResponse), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    throw new Error(`unexpected fetch URL in test: ${url}`);
  });
  // @ts-expect-error overriding the global is intentional for test isolation
  globalThis.fetch = mockFetch;
  return mockFetch;
}

describe("clientAuthHeader: refresh-token rotation", () => {
  let tmpDir: string;
  let credentialsPath: string;
  let originalFetch: typeof fetch;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), "vorpal-sdk-test-"));
    credentialsPath = join(tmpDir, "credentials.json");
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    rmSync(tmpDir, { recursive: true, force: true });
  });

  test("persists rotated refresh_token returned by the IdP", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "old-refresh-token" });
    installFetchMock({
      tokenResponse: {
        access_token: "new-access-token",
        expires_in: 3600,
        refresh_token: "rotated-refresh-token",
      },
    });

    const header = await clientAuthHeader(REGISTRY, credentialsPath);

    expect(header).toBe("Bearer new-access-token");

    const persisted = JSON.parse(readFileSync(credentialsPath, "utf-8"));
    expect(persisted.issuer[ISSUER].refresh_token).toBe("rotated-refresh-token");
    expect(persisted.issuer[ISSUER].access_token).toBe("new-access-token");
    // Unrelated fields must be preserved verbatim.
    expect(persisted.issuer[ISSUER].audience).toBe("vorpal");
    expect(persisted.issuer[ISSUER].client_id).toBe("vorpal-cli");
    expect(persisted.issuer[ISSUER].scopes).toEqual(["openid", "offline_access"]);
  });

  test("leaves existing refresh_token untouched when IdP omits one", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "old-refresh-token" });
    installFetchMock({
      tokenResponse: {
        access_token: "new-access-token",
        expires_in: 3600,
        // No refresh_token in the response — IdP did not rotate.
      },
    });

    const header = await clientAuthHeader(REGISTRY, credentialsPath);

    expect(header).toBe("Bearer new-access-token");

    const persisted = JSON.parse(readFileSync(credentialsPath, "utf-8"));
    expect(persisted.issuer[ISSUER].refresh_token).toBe("old-refresh-token");
    expect(persisted.issuer[ISSUER].access_token).toBe("new-access-token");
  });

  test("leaves existing refresh_token untouched when IdP returns empty string", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "old-refresh-token" });
    installFetchMock({
      tokenResponse: {
        access_token: "new-access-token",
        expires_in: 3600,
        refresh_token: "",
      },
    });

    await clientAuthHeader(REGISTRY, credentialsPath);

    const persisted = JSON.parse(readFileSync(credentialsPath, "utf-8"));
    expect(persisted.issuer[ISSUER].refresh_token).toBe("old-refresh-token");
  });

  test("does not rewrite credentials when token is still valid", async () => {
    const now = Math.floor(Date.now() / 1000);
    writeCredentialsFile(credentialsPath, {
      issuedAt: now,
      expiresIn: 3600,
      refreshToken: "untouched-refresh-token",
    });
    const fetchMock = installFetchMock({
      tokenResponse: {
        access_token: "should-not-be-used",
        refresh_token: "should-not-be-persisted",
      },
    });
    const before = readFileSync(credentialsPath, "utf-8");

    const header = await clientAuthHeader(REGISTRY, credentialsPath);

    expect(header).toBe("Bearer old-access-token");
    expect(fetchMock).not.toHaveBeenCalled();
    const after = readFileSync(credentialsPath, "utf-8");
    expect(after).toBe(before);
  });
});

const NON_UTF8_SECRET = Uint8Array.from([0xff, 0x00, 0xfe, 0x3d, 0x0a]);

// Mirrors the fixture in sdk/rust/src/serde_base64.rs so digests compare across SDKs.
function artifactWithSecrets(secrets: ArtifactStepSecret[]): ArtifactMsg {
  return {
    aliases: ["example:latest"],
    name: "example",
    sources: [
      {
        digest: "abc123",
        excludes: ["target"],
        includes: ["src"],
        name: "example",
        path: ".",
      },
    ],
    steps: [
      {
        arguments: ["-c"],
        artifacts: ["def456"],
        entrypoint: "bash",
        environments: ["PATH=/bin"],
        script: "echo hi",
        secrets,
      },
    ],
    systems: [ArtifactSystem.AARCH64_DARWIN],
    target: ArtifactSystem.AARCH64_DARWIN,
  };
}

describe("step secret values", () => {
  test("serializes the value as padded standard base64", () => {
    const secret = { name: "TOKEN", value: Buffer.from(NON_UTF8_SECRET) };

    expect(JSON.stringify(serializeArtifactStepSecret(secret))).toBe(
      '{"name":"TOKEN","value":"/wD+PQo="}',
    );
  });

  test("artifact JSON with a secret matches the Rust SDK's text", () => {
    const rustJson =
      '{"target":1,"sources":[],"steps":[{"entrypoint":"bash","script":"echo hi","secrets":[{"name":"TOKEN","value":"aHVudGVyMg=="}],"arguments":[],"artifacts":[],"environments":[]}],"systems":[1],"aliases":[],"name":"example"}';
    const artifact: ArtifactMsg = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "echo hi",
          secrets: [{ name: "TOKEN", value: Buffer.from("hunter2") }],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "example",
    };

    expect(artifactToJsonBytes(artifact).toString("utf-8")).toBe(rustJson);
  });

  test("artifact without secrets keeps its digest", () => {
    expect(computeArtifactDigest(artifactWithSecrets([]))).toBe(
      "71bd330295891ae4a37684e7eb10f12bfbfb6fa0058db0f357c00b747fd752fc",
    );
  });

  test("secretsToProto stores text as its UTF-8 bytes and bytes as given, sorted by name", () => {
    const secrets = new Map<string, string | Uint8Array>([
      ["TOKEN", "hunter2"],
      ["BINARY", NON_UTF8_SECRET],
    ]);

    const proto = secretsToProto(secrets);

    expect(proto.map((s) => s.name)).toEqual(["BINARY", "TOKEN"]);
    expect([...proto[0]!.value]).toEqual([...NON_UTF8_SECRET]);
    expect([...proto[1]!.value]).toEqual([0x68, 0x75, 0x6e, 0x74, 0x65, 0x72, 0x32]);
  });

  test("ArtifactStep builds a step message holding the bytes passed in", () => {
    const step = new ArtifactStep("bash")
      .withSecrets(secretsToProto(new Map([["BINARY", NON_UTF8_SECRET]])))
      .build();

    expect(step.secrets).toHaveLength(1);
    expect(step.secrets[0]!.name).toBe("BINARY");
    expect([...step.secrets[0]!.value]).toEqual([...NON_UTF8_SECRET]);
  });
});
