import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { clientAuthHeader, credentialEgressOrigin, needsRefresh, refreshAccessToken } from "../context.js";

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
  /** Status code for the token endpoint response. Defaults to 200. */
  tokenStatus?: number;
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
        status: scenario.tokenStatus ?? 200,
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
    writeCredentialsFile(credentialsPath, { refreshToken: "rotation-old-refresh-token" });
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
    writeCredentialsFile(credentialsPath, { refreshToken: "omit-old-refresh-token" });
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
    expect(persisted.issuer[ISSUER].refresh_token).toBe("omit-old-refresh-token");
    expect(persisted.issuer[ISSUER].access_token).toBe("new-access-token");
  });

  test("leaves existing refresh_token untouched when IdP returns empty string", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "empty-old-refresh-token" });
    installFetchMock({
      tokenResponse: {
        access_token: "new-access-token",
        expires_in: 3600,
        refresh_token: "",
      },
    });

    await clientAuthHeader(REGISTRY, credentialsPath);

    const persisted = JSON.parse(readFileSync(credentialsPath, "utf-8"));
    expect(persisted.issuer[ISSUER].refresh_token).toBe("empty-old-refresh-token");
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

  // ---------------------------------------------------------------------
  // C-1 / C-2 — serialization and post-lock clock sampling (AB-1, AB-2)
  // ---------------------------------------------------------------------

  test("serializes concurrent refreshes into exactly one exchange", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "concurrent-old-refresh-token" });
    let calls = 0;
    installFetchMockCounting(() => {
      calls += 1;
      return { access_token: "new-access-token", expires_in: 3600 };
    });

    const results = await Promise.all(
      Array.from({ length: 8 }, () => clientAuthHeader(REGISTRY, credentialsPath)),
    );

    for (const header of results) {
      expect(header).toBe("Bearer new-access-token");
    }
    expect(calls).toBe(1);
  });

  function installFetchMockCounting(tokenResponse: () => Record<string, unknown>): void {
    const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === TOKEN_ENDPOINT) {
        return new Response(JSON.stringify(tokenResponse()), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;
  }

  // ---------------------------------------------------------------------
  // C-3 / C-4 — spent-token memo and NotSent/Sent classification (AB-1, AB-3, AB-4)
  // ---------------------------------------------------------------------

  test("never replays a token after a Sent failure", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "sent-fail-refresh-token" });
    installFetchMock({ tokenResponse: { error: "server_error" }, tokenStatus: 500 });

    await expect(clientAuthHeader(REGISTRY, credentialsPath)).rejects.toThrow();

    const secondError = await clientAuthHeader(REGISTRY, credentialsPath).catch((e) => e as Error);
    expect(secondError).toBeInstanceOf(Error);
    expect((secondError as Error).message).toContain("already failed");
  });

  test("retries a refresh that never reached the IdP", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "not-sent-refresh-token" });

    let attempt = 0;
    const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        attempt += 1;
        if (attempt === 1) {
          throw new Error("simulated DNS failure");
        }
        return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === TOKEN_ENDPOINT) {
        return new Response(
          JSON.stringify({ access_token: "new-access-token", expires_in: 3600 }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;

    const firstError = await clientAuthHeader(REGISTRY, credentialsPath).catch((e) => e as Error);
    expect(firstError).toBeInstanceOf(Error);
    expect((firstError as Error).message).not.toContain("already failed");

    const header = await clientAuthHeader(REGISTRY, credentialsPath);
    expect(header).toBe("Bearer new-access-token");
  });

  // ---------------------------------------------------------------------
  // C-6 / C-7 — skew-safe age and proportional window (AB-7, AB-8)
  // ---------------------------------------------------------------------

  test("refreshes despite a future-dated issued_at", async () => {
    const now = Math.floor(Date.now() / 1000);
    writeCredentialsFile(credentialsPath, {
      issuedAt: now + 3600,
      expiresIn: 3600,
      refreshToken: "future-issued-refresh-token",
    });
    let calls = 0;
    installFetchMockCounting(() => {
      calls += 1;
      return { access_token: "new-access-token", expires_in: 3600 };
    });

    const header = await clientAuthHeader(REGISTRY, credentialsPath);

    expect(header).toBe("Bearer new-access-token");
    expect(calls).toBe(1);
  });

  // ---------------------------------------------------------------------
  // C-10 — refuse an unusable token lifetime (AB-11)
  // ---------------------------------------------------------------------

  test("refuses to persist a zero-lifetime token", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "zero-lifetime-refresh-token" });
    installFetchMock({ tokenResponse: { access_token: "new-access-token", expires_in: 0 } });

    await expect(clientAuthHeader(REGISTRY, credentialsPath)).rejects.toThrow();

    const persisted = JSON.parse(readFileSync(credentialsPath, "utf-8"));
    expect(persisted.issuer[ISSUER].access_token).toBe("old-access-token");
  });

  // ---------------------------------------------------------------------
  // C-11 — validate the credential record at first contact (AB-7b, AB-12)
  // ---------------------------------------------------------------------

  test("rejects a non-numeric issued_at rather than suppressing refresh via NaN", async () => {
    const raw = JSON.stringify({
      issuer: {
        [ISSUER]: {
          access_token: "old-access-token",
          client_id: "vorpal-cli",
          expires_in: 3600,
          issued_at: "corrupt",
          refresh_token: "corrupt-issued-at-refresh-token",
          scopes: [],
        },
      },
      registry: { [REGISTRY]: ISSUER },
    });
    writeFileSync(credentialsPath, raw, { mode: 0o600 });

    await expect(clientAuthHeader(REGISTRY, credentialsPath)).rejects.toThrow();
  });

  test("rejects a negative expires_in", async () => {
    writeCredentialsFile(credentialsPath, { expiresIn: -1, refreshToken: "negative-expires-refresh-token" });

    await expect(clientAuthHeader(REGISTRY, credentialsPath)).rejects.toThrow();
  });

  // ---------------------------------------------------------------------
  // C-14 — no token material in errors
  // ---------------------------------------------------------------------

  test("error messages never contain the refresh token value", async () => {
    const secretToken = "super-secret-refresh-token-value";
    writeCredentialsFile(credentialsPath, { refreshToken: secretToken });
    installFetchMock({ tokenResponse: { error: "server_error" }, tokenStatus: 500 });

    const err = await clientAuthHeader(REGISTRY, credentialsPath).catch((e) => e as Error);
    expect(err).toBeInstanceOf(Error);
    expect((err as Error).message).not.toContain(secretToken);
  });

  // ---------------------------------------------------------------------
  // C-4 — a malformed token-endpoint body must not escape Sent classification
  // (VPL-189-CL1/CL2). Before this fix, a literal `null` body passed
  // `tokenResp.json()` (it is valid JSON) and then threw an unclassified
  // TypeError reading `.refresh_token` off `null` — the token was already
  // on the wire, but the failure was never marked Sent, so a second caller
  // replayed it against the IdP instead of getting a "please re-login"
  // error.
  // ---------------------------------------------------------------------

  test("classifies a null token-endpoint body as Sent rather than replaying", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "malformed-body-refresh-token" });
    const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === TOKEN_ENDPOINT) {
        // Valid JSON, wrong shape: `tokenResp.json()` resolves successfully
        // with `null`, so only a shape check (not the parse try/catch
        // alone) can classify this as Sent.
        return new Response("null", { status: 200, headers: { "content-type": "application/json" } });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;

    const first = await clientAuthHeader(REGISTRY, credentialsPath).catch((e) => e as Error);
    expect(first).toBeInstanceOf(Error);

    const second = await clientAuthHeader(REGISTRY, credentialsPath).catch((e) => e as Error);
    expect(second).toBeInstanceOf(Error);
    expect((second as Error).message).toContain("already failed");
  });

  test("refuses to persist a token response missing access_token", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "missing-access-token-refresh-token" });
    installFetchMock({ tokenResponse: { expires_in: 3600 } });

    await expect(clientAuthHeader(REGISTRY, credentialsPath)).rejects.toThrow();

    const persisted = JSON.parse(readFileSync(credentialsPath, "utf-8"));
    expect(persisted.issuer[ISSUER].access_token).toBe("old-access-token");
  });

  // ---------------------------------------------------------------------
  // C-3 — a commit failure after a successful exchange must spend the
  // token (AB-3), same as Go's equivalent chmod-based test.
  // ---------------------------------------------------------------------

  test("never replays a token whose refresh succeeded but failed to persist", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "persist-fail-refresh-token" });
    installFetchMock({ tokenResponse: { access_token: "new-access-token", expires_in: 3600 } });

    chmodSync(tmpDir, 0o500);
    try {
      const first = await clientAuthHeader(REGISTRY, credentialsPath).catch((e) => e as Error);
      expect(first).toBeInstanceOf(Error);
    } finally {
      chmodSync(tmpDir, 0o700);
    }

    const second = await clientAuthHeader(REGISTRY, credentialsPath).catch((e) => e as Error);
    expect(second).toBeInstanceOf(Error);
    expect((second as Error).message).toContain("already failed");
  });

  // ---------------------------------------------------------------------
  // C-2 — the clock must be sampled after the lock is held, not before
  // (AB-2 waiter storm). With the wall clock as `now` and a deliberately
  // slow winner exchange, a pre-lock sample would be provably stale by the
  // time a waiter gets the lock: the winner's freshly committed issued_at
  // (stamped near the end of the delay) would postdate that stale reading,
  // needsRefresh's future-dated-issued_at rule would read it as due, and
  // every waiter would refresh again. A post-lock sample reads the winner's
  // already-committed, now-fresh state and finds no refresh due.
  // ---------------------------------------------------------------------

  test("samples the clock after the lock, so concurrent waiters never over-refresh", async () => {
    const now = Math.floor(Date.now() / 1000);
    writeCredentialsFile(credentialsPath, { issuedAt: now - 7200, refreshToken: "clock-after-lock-refresh-token" });
    let calls = 0;
    const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === TOKEN_ENDPOINT) {
        calls += 1;
        // Deliberately slow: gives every other concurrent caller a chance
        // to have already sampled `now` before the winner commits, if the
        // implementation samples it before acquiring the lock.
        await new Promise((resolve) => setTimeout(resolve, 50));
        return new Response(JSON.stringify({ access_token: "new-access-token", expires_in: 3600 }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;

    const wallClockNow = () => Math.floor(Date.now() / 1000);
    const results = await Promise.all(
      Array.from({ length: 8 }, () =>
        clientAuthHeader(REGISTRY, credentialsPath, refreshAccessToken, wallClockNow),
      ),
    );

    for (const header of results) {
      expect(header).toBe("Bearer new-access-token");
    }
    expect(calls).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// C-5 — atomic, mode-enforcing write (AB-5, AB-6)
// ---------------------------------------------------------------------------

describe("clientAuthHeader: atomic credential write", () => {
  let tmpDir: string;
  let credentialsPath: string;
  let originalFetch: typeof fetch;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), "vorpal-sdk-test-write-"));
    credentialsPath = join(tmpDir, "credentials.json");
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    rmSync(tmpDir, { recursive: true, force: true });
  });

  test("overwrites a pre-existing loose file mode to 0600", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "mode-refresh-token" });
    // Simulate a credentials file that pre-dates this control.
    chmodSync(credentialsPath, 0o644);
    expect(statSync(credentialsPath).mode & 0o777).toBe(0o644);

    installFetchMock({ tokenResponse: { access_token: "new-access-token", expires_in: 3600 } });
    await clientAuthHeader(REGISTRY, credentialsPath);

    expect(statSync(credentialsPath).mode & 0o777).toBe(0o600);
  });

  function installFetchMock(scenario: FetchScenario): void {
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
          status: scenario.tokenStatus ?? 200,
          headers: { "content-type": "application/json" },
        });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;
  }

  test("a reader looping on parse never observes a torn file", async () => {
    writeCredentialsFile(credentialsPath, { refreshToken: "reader-writer-refresh-token" });

    let stop = false;
    let readError: Error | undefined;
    const readerLoop = (async () => {
      while (!stop) {
        try {
          const data = readFileSync(credentialsPath, "utf-8");
          JSON.parse(data);
        } catch (err) {
          if (err instanceof SyntaxError) {
            readError = err;
            return;
          }
          // ENOENT during the rename window is not a torn read.
        }
        await new Promise((resolve) => setImmediate(resolve));
      }
    })();

    let n = 0;
    installFetchMockCounting(() => {
      n += 1;
      return { access_token: `access-${n}`, expires_in: 3600, refresh_token: `reader-writer-refresh-token-${n}` };
    });

    for (let i = 0; i < 30; i++) {
      const now = Math.floor(Date.now() / 1000) - 7200;
      writeCredentialsFile(credentialsPath, { issuedAt: now, refreshToken: `reader-writer-refresh-token-${i}` });
      await clientAuthHeader(REGISTRY, credentialsPath);
    }

    stop = true;
    await readerLoop;

    expect(readError).toBeUndefined();
  });

  function installFetchMockCounting(tokenResponse: () => Record<string, unknown>): void {
    const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === TOKEN_ENDPOINT) {
        return new Response(JSON.stringify(tokenResponse()), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;
  }
});

// ---------------------------------------------------------------------------
// C-6 / C-7 — needsRefresh unit behavior
// ---------------------------------------------------------------------------

describe("needsRefresh", () => {
  test("treats a future issued_at as unknown age", () => {
    const now = 1_000_000;
    expect(needsRefresh(now + 3600, 3600, now)).toBe(true);
  });

  test.each([1, 2, 60, 299, 300, 301, 600, 3600])(
    "is never due when just issued, and due at its window boundary (expiresIn=%d)",
    (expiresIn) => {
      const issuedAt = 1_000_000;
      const window = Math.min(300, Math.floor(expiresIn / 2));

      expect(needsRefresh(issuedAt, expiresIn, issuedAt)).toBe(false);

      const boundaryNow = issuedAt + expiresIn - window;
      expect(needsRefresh(issuedAt, expiresIn, boundaryNow)).toBe(true);
    },
  );
});

// ---------------------------------------------------------------------------
// C-8 — egress origin validation (AB-9, AB-10 discovery arm)
// ---------------------------------------------------------------------------

describe("credentialEgressOrigin", () => {
  test("refuses a non-loopback http issuer", () => {
    expect(() => credentialEgressOrigin("http://idp.example.com")).toThrow();
  });

  test("allows loopback http", () => {
    expect(credentialEgressOrigin("http://127.0.0.1:8080")).toBe("http://127.0.0.1:8080");
  });

  test("normalizes the default https port", () => {
    expect(credentialEgressOrigin("https://idp.example.com")).toBe("https://idp.example.com:443");
  });

  // WHATWG URL keeps the bracket syntax in `.hostname` for an IPv6
  // literal ("[::1]", not "::1"), unlike Go's url.Hostname() — the
  // loopback comparison must strip it or every IPv6-loopback issuer is
  // wrongly refused as non-loopback.
  test("allows IPv6 loopback http", () => {
    expect(credentialEgressOrigin("http://[::1]:8080")).toBe("http://::1:8080");
  });
});

describe("refreshAccessToken: egress and redirect controls", () => {
  let originalFetch: typeof fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  test("refuses a non-https issuer before making any request", async () => {
    const mockFetch = mock(async () => {
      throw new Error("must not be called");
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;

    await expect(
      refreshAccessToken(undefined, "client", "http://idp.example.com", "refresh-token"),
    ).rejects.toThrow();
    expect(mockFetch).not.toHaveBeenCalled();
  });

  test("does not double the slash discovering an issuer with a trailing slash", async () => {
    const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === TOKEN_ENDPOINT) {
        return new Response(JSON.stringify({ access_token: "new-access-token", expires_in: 3600 }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;

    const result = await refreshAccessToken(undefined, "client", `${ISSUER}/`, "refresh-token");
    expect(result.accessToken).toBe("new-access-token");
  });

  test("refuses a token_endpoint on a different origin than the issuer", async () => {
    let otherCalled = false;
    const mockFetch = mock(async (input: RequestInfo | URL): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        return new Response(JSON.stringify({ token_endpoint: "https://attacker.example/token" }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === "https://attacker.example/token") {
        otherCalled = true;
        return new Response("{}", { status: 200 });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;

    await expect(refreshAccessToken(undefined, "client", ISSUER, "refresh-token")).rejects.toThrow();
    expect(otherCalled).toBe(false);
  });

  // C-9: redirect ban. `redirect: "error"` (set on every request refreshAccessToken
  // issues) makes fetch reject rather than follow — the positive control for
  // why this matters is that Node/Bun's *default* fetch follows a 307 and
  // resends the credential-bearing POST body verbatim (REPRODUCED during the
  // VPL-189 threat model against a local pair of httptest-equivalent servers).
  test("refuses a redirect from the token endpoint", async () => {
    let secondCalled = false;
    const mockFetch = mock(async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const url = typeof input === "string" ? input : input.toString();
      if (url === `${ISSUER}/.well-known/openid-configuration`) {
        return new Response(JSON.stringify({ token_endpoint: TOKEN_ENDPOINT }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (url === TOKEN_ENDPOINT) {
        if (init?.redirect === "error") {
          // Faithful to real fetch semantics: a redirect response under
          // redirect:"error" is surfaced as a rejected promise, not a 3xx
          // Response.
          throw new TypeError("unexpected redirect");
        }
        secondCalled = true;
        return new Response("{}", { status: 200 });
      }
      throw new Error(`unexpected fetch URL in test: ${url}`);
    });
    // @ts-expect-error overriding the global is intentional for test isolation
    globalThis.fetch = mockFetch;

    await expect(refreshAccessToken(undefined, "client", ISSUER, "refresh-token")).rejects.toThrow();
    expect(secondCalled).toBe(false);
  });
});
