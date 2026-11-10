export default {
  async fetch(request, env) {
    const key = decodeURIComponent(new URL(request.url).pathname.slice(1));
    const object = await env.LARGE_FILES.get(key, { onlyIf: request.headers });
    if (object === null) return new Response("Not found", { status: 404 });
    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set("etag", object.httpEtag);
    if (!("body" in object)) return new Response(null, { status: 304, headers });
    return new Response(request.method === "HEAD" ? null : object.body, { headers });
  },
};
