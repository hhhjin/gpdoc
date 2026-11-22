import type { APIRoute, GetStaticPaths } from "astro";

export const getStaticPaths = (() => {
  const modules = import.meta.glob<string>("../../../../../plugins/*/src/**/*.rs", {
    eager: true,
    query: "?raw",
    import: "default",
  });

  return Object.entries(modules).map(([path, source]) => ({
    params: { file: path.split("/plugins/")[1] },
    props: { source },
  }));
}) satisfies GetStaticPaths;

export const GET: APIRoute<{ source: string }> = ({ props }) =>
  new Response(props.source, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
