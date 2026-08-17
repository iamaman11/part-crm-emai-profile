export const GITHUB_READ_RETRY_POLICY = Object.freeze({
  maxAttempts: 5,
  baseDelayMs: 1_000,
  maxDelayMs: 8_000,
});

export function isRetryableGitHubReadStatus(status) {
  return status === 429 || (Number.isInteger(status) && status >= 500 && status <= 599);
}

export function isRetryableGitHubTransportError(error) {
  return error instanceof TypeError || error?.name === 'AbortError' || error?.name === 'TimeoutError';
}

function retryAfterDelayMs(response, nowMs = Date.now()) {
  const raw = response?.headers?.get?.('retry-after');
  if (typeof raw !== 'string' || raw.trim().length === 0) return null;
  const value = raw.trim();
  if (/^\d+$/.test(value)) return Number(value) * 1_000;
  const targetMs = Date.parse(value);
  if (!Number.isFinite(targetMs)) return null;
  return Math.max(0, targetMs - nowMs);
}

export function githubReadRetryDelayMs(response, attempt, nowMs = Date.now()) {
  const retryAfterMs = retryAfterDelayMs(response, nowMs);
  if (retryAfterMs !== null) return Math.min(retryAfterMs, GITHUB_READ_RETRY_POLICY.maxDelayMs);
  const exponential = GITHUB_READ_RETRY_POLICY.baseDelayMs * (2 ** Math.max(0, attempt - 1));
  return Math.min(exponential, GITHUB_READ_RETRY_POLICY.maxDelayMs);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function safeMessage(value) {
  if (typeof value !== 'string' || value.length === 0) return 'unknown error';
  return value.replace(/[\r\n\t]/g, ' ').slice(0, 300);
}

export async function githubReadJson({ token, path, apiVersion, userAgent, timeoutMs = 20_000, fetchImpl = fetch, sleepImpl = sleep }) {
  for (let attempt = 1; attempt <= GITHUB_READ_RETRY_POLICY.maxAttempts; attempt += 1) {
    let response;
    try {
      response = await fetchImpl(`https://api.github.com${path}`, {
        headers: {
          Authorization: `Bearer ${token}`,
          Accept: 'application/vnd.github+json',
          'X-GitHub-Api-Version': apiVersion,
          'User-Agent': userAgent,
        },
        signal: AbortSignal.timeout(timeoutMs),
      });
    } catch (error) {
      if (!isRetryableGitHubTransportError(error) || attempt === GITHUB_READ_RETRY_POLICY.maxAttempts) {
        throw new Error(`GitHub ${path} transport failure after ${attempt} attempt(s): ${safeMessage(error?.message)}`);
      }
      await sleepImpl(githubReadRetryDelayMs(null, attempt));
      continue;
    }

    const text = await response.text();
    let payload = null;
    if (text) {
      try {
        payload = JSON.parse(text);
      } catch {
        if (isRetryableGitHubReadStatus(response.status) && attempt < GITHUB_READ_RETRY_POLICY.maxAttempts) {
          await sleepImpl(githubReadRetryDelayMs(response, attempt));
          continue;
        }
        throw new Error(`GitHub ${path} returned non-JSON HTTP ${response.status}`);
      }
    }

    if (response.ok) return payload;

    const message = safeMessage(payload?.message);
    if (!isRetryableGitHubReadStatus(response.status) || attempt === GITHUB_READ_RETRY_POLICY.maxAttempts) {
      throw new Error(`GitHub ${path} failed with HTTP ${response.status} after ${attempt} attempt(s): ${message}`);
    }
    await sleepImpl(githubReadRetryDelayMs(response, attempt));
  }

  throw new Error(`GitHub ${path} exhausted the bounded retry policy`);
}
