import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { clientAuthHeader } from "../context.js";

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
