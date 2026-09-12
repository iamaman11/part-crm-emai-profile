import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import { spawn } from "node:child_process";
import { generateKeyPairSync, sign } from "node:crypto";

const required = (name) => {
  const value = process.env[name];
  if (!value) throw new Error(`missing ${name}`);
  return value;
};

const controlPort = Number.parseInt(required("E2E_CONTROL_PORT"), 10);
const dependencyPort = Number.parseInt(required("E2E_DEPENDENCY_PORT"), 10);
const ingressPort = Number.parseInt(required("E2E_INGRESS_PORT"), 10);
const pfxPath = required("E2E_TLS_PFX");
const pfxPassword = required("E2E_TLS_PFX_PASSWORD");
const tokenFile = required("E2E_TOKEN_FILE");
const signerScript = required("E2E_SIGNER_SCRIPT");
const issuer = `http://127.0.0.1:${dependencyPort}`;
const audience = "bridge-enrollment-e2e-audience";
const subject = "bridge_e2e_subject_01";
const keyId = "bridge-e2e-key-01";
const MAX_BODY_BYTES = 512 * 1024;

for (const [name, port] of [
  ["E2E_CONTROL_PORT", controlPort],
  ["E2E_DEPENDENCY_PORT", dependencyPort],
  ["E2E_INGRESS_PORT", ingressPort],
]) {
  if (!Number.isInteger(port) || port < 1024 || port > 65535) {
    throw new Error(`invalid ${name}`);
  }
}

const base64url = (value) => Buffer.from(value).toString("base64url");
const keyPair = generateKeyPairSync("rsa", { modulusLength: 2048 });
const publicJwk = keyPair.publicKey.export({ format: "jwk" });
const jwks = {
  keys: [
    {
      kid: keyId,
      kty: "RSA",
      alg: "RS256",
      use: "sig",
      n: publicJwk.n,
      e: publicJwk.e,
    },
  ],
};
const now = Math.floor(Date.now() / 1000);
const header = base64url(JSON.stringify({ alg: "RS256", kid: keyId }));
const payload = base64url(
  JSON.stringify({
    iss: issuer,
    aud: audience,
    sub: subject,
    email: "bridge-e2e@example.test",
    nbf: now - 60,
    exp: now + 3600,
  }),
);
const signingInput = `${header}.${payload}`;
const signature = sign("RSA-SHA256", Buffer.from(signingInput), keyPair.privateKey).toString("base64url");
fs.writeFileSync(tokenFile, `${signingInput}.${signature}`, { encoding: "utf8", mode: 0o600 });

const readBody = async (request) => {
  const chunks = [];
  let length = 0;
  for await (const chunk of request) {
    length += chunk.length;
    if (length > MAX_BODY_BYTES) throw new Error("request body too large");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
};

const jsonResponse = (response, status, value) => {
  const body = Buffer.from(JSON.stringify(value));
  response.writeHead(status, {
    "content-type": "application/json",
    "cache-control": "no-store",
    "content-length": body.length,
  });
  response.end(body);
};

let failNextSignerRequest = false;

const runSigner = (body) =>
  new Promise((resolve, reject) => {
    const child = spawn(
      "pwsh.exe",
      ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", signerScript],
      { stdio: ["pipe", "pipe", "pipe"], windowsHide: true },
    );
    const stdout = [];
    const stderr = [];
    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) {
        process.stderr.write(`local signer failed with exit ${code}\n`);
        reject(new Error("local signer rejected request"));
        return;
      }
      const output = Buffer.concat(stdout);
      if (output.length === 0 || output.length > MAX_BODY_BYTES) {
        reject(new Error("local signer returned invalid output"));
        return;
      }
      resolve(output);
    });
    child.stdin.end(body);
  });

const dependencyServer = http.createServer(async (request, response) => {
  try {
    const url = new URL(request.url, issuer);
    if (request.method === "GET" && url.pathname === "/cdn-cgi/access/certs") {
      jsonResponse(response, 200, jwks);
      return;
    }
    if (request.method === "POST" && url.pathname === "/__e2e/fail-next-signer") {
      failNextSignerRequest = true;
      response.writeHead(204, { "cache-control": "no-store" });
      response.end();
      return;
    }
    if (request.method === "POST" && url.pathname === "/sign") {
      const body = await readBody(request);
      if (failNextSignerRequest) {
        failNextSignerRequest = false;
        jsonResponse(response, 503, { code: "test_dependency_unavailable" });
        return;
      }
      try {
        const signed = await runSigner(body);
        response.writeHead(200, {
          "content-type": "application/json",
          "cache-control": "no-store",
          "content-length": signed.length,
        });
        response.end(signed);
      } catch {
        jsonResponse(response, 422, { code: "test_signer_rejected" });
      }
      return;
    }
    response.writeHead(404, { "cache-control": "no-store" });
    response.end("Not Found");
  } catch {
    jsonResponse(response, 500, { code: "test_dependency_failure" });
  }
});

const proxyToControlPlane = async (request, response) => {
  if (request.method === "GET" && request.url === "/__e2e/ready") {
    response.writeHead(200, { "content-type": "text/plain", "cache-control": "no-store" });
    response.end("ready");
    return;
  }

  let body;
  try {
    body = await readBody(request);
  } catch {
    response.writeHead(413, { "cache-control": "no-store" });
    response.end();
    return;
  }
  const headers = { ...request.headers };
  const accessToken = headers["cf-access-token"];
  delete headers.host;
  delete headers.connection;
  delete headers["content-length"];
  delete headers["cf-access-token"];
  if (typeof accessToken === "string" && accessToken.length > 0) {
    headers["cf-access-jwt-assertion"] = accessToken;
  }
  headers["content-length"] = String(body.length);

  const upstream = http.request(
    {
      hostname: "127.0.0.1",
      port: controlPort,
      path: request.url,
      method: request.method,
      headers,
    },
    (upstreamResponse) => {
      const forwardedHeaders = {};
      for (const name of ["content-type", "cache-control"]) {
        const value = upstreamResponse.headers[name];
        if (value !== undefined) forwardedHeaders[name] = value;
      }
      response.writeHead(upstreamResponse.statusCode ?? 502, forwardedHeaders);
      upstreamResponse.pipe(response);
    },
  );
  upstream.on("error", () => {
    if (!response.headersSent) response.writeHead(502, { "cache-control": "no-store" });
    response.end();
  });
  upstream.end(body);
};

const ingressServer = https.createServer(
  { pfx: fs.readFileSync(pfxPath), passphrase: pfxPassword, minVersion: "TLSv1.2" },
  (request, response) => void proxyToControlPlane(request, response),
);

dependencyServer.listen(dependencyPort, "127.0.0.1", () => {
  process.stdout.write(`dependency-ready:${dependencyPort}\n`);
});
ingressServer.listen(ingressPort, "127.0.0.1", () => {
  process.stdout.write(`ingress-ready:${ingressPort}\n`);
});

const shutdown = () => {
  ingressServer.close();
  dependencyServer.close(() => process.exit(0));
  setTimeout(() => process.exit(0), 1000).unref();
};
process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);
