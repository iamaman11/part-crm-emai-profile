const MAX_BODY_BYTES = 512 * 1024;

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (request.method !== "POST" || url.pathname !== "/v1/bridge-enrollment/sign") {
      return new Response("Not Found", { status: 404 });
    }
    const contentType = request.headers.get("content-type");
    if (contentType?.split(";", 1)[0].trim().toLowerCase() !== "application/json") {
      return new Response("Invalid Request", { status: 422 });
    }

    const body = await request.arrayBuffer();
    if (body.byteLength === 0 || body.byteLength > MAX_BODY_BYTES) {
      return new Response("Invalid Request", { status: 422 });
    }

    const target = new URL("/sign", env.TEST_SIGNER_ORIGIN);
    const response = await fetch(target, {
      method: "POST",
      headers: {
        accept: "application/json",
        "content-type": "application/json",
        "cache-control": "no-store",
      },
      body,
    });
    return new Response(response.body, {
      status: response.status,
      headers: {
        "content-type": response.headers.get("content-type") ?? "application/json",
        "cache-control": "no-store",
      },
    });
  },
};
