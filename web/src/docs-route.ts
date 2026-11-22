import { defineRouteMiddleware } from "@astrojs/starlight/route-data";

export const onRequest = defineRouteMiddleware((context) => {
  context.locals.starlightRoute.siteTitleHref = `${import.meta.env.BASE_URL}docs/`;
});
