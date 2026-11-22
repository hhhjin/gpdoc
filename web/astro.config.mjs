import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import starlightThemeBlack from "starlight-theme-black";
import { pluginGroups } from "./src/plugin-groups.mjs";

export default defineConfig({
  site: "https://gpdoc.dev",
  devToolbar: { enabled: false },
  integrations: [
    starlight({
      plugins: [
        starlightThemeBlack({}),
        {
          name: "gpdoc-theme-toggle",
          hooks: {
            "config:setup": ({ config, updateConfig }) => {
              updateConfig({ components: { ...config.components, ThemeSelect: "./src/components/ThemeToggle.astro" } });
            },
          },
        },
      ],
      title: "gpdoc",
      description: "Build rich-text editors with Rust and GPUI.",
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/hhhjin/gpdot" }],
      favicon: "/favicon.png",
      head: [
        {
          tag: "style",
          content: `
            header .social-icons:empty {
              display: none;
            }

            @media (min-width: 64rem) {
              /* Starlight restores scrollTop on the pane when navigating between docs. */
              #starlight__sidebar {
                height: auto;
                bottom: 0;
                overflow-y: auto;
                scrollbar-width: none;
                background-image: linear-gradient(transparent, var(--border), transparent);
                background-position: right top;
                background-repeat: no-repeat;
                background-size: 1px 100%;
              }

              #starlight__sidebar::after {
                display: none;
              }

              #starlight__sidebar > .sidebar-content::after {
                content: none;
              }

              #starlight__sidebar > .sidebar-content {
                padding-bottom: 1.5rem;
                height: auto;
                mask-image: none;
                animation: none;
                overflow: visible;
                flex: none;
              }
            }

            ul.top-level a.entry-link {
              width: 100%;
              height: auto;
              min-height: 30px;
              padding-block: 0.375rem;
              line-height: 1.4;
              overflow-wrap: anywhere;
            }

            ul.top-level a.entry-link > .entry-link-inner {
              width: auto;
            }

            @media (prefers-reduced-motion: no-preference) {
              @view-transition {
                navigation: auto;
              }
            }
          `,
        },
      ],
      routeMiddleware: "./src/docs-route.ts",
      sidebar: [
        {
          label: "Getting started",
          items: [
            { label: "Overview", slug: "docs" },
            { slug: "docs/quickstart" },
          ],
        },
        {
          label: "Plugins",
          items: [
            { label: "Browse plugins", slug: "docs/plugins" },
            ...pluginGroups.map(({ label, pages }) => ({
              label,
              collapsed: true,
              items: pages.map((page) => ({ slug: `docs/plugins/${page}` })),
            })),
            {
              label: "Build a plugin",
              collapsed: true,
              items: [
                { slug: "docs/plugins/custom/first-plugin" },
                { slug: "docs/plugins/custom/schema" },
                { slug: "docs/plugins/custom/commands" },
                { slug: "docs/plugins/custom/state" },
                { slug: "docs/plugins/custom/input-and-paste" },
                { slug: "docs/plugins/custom/gpui" },
                { slug: "docs/plugins/custom/api" },
              ],
            },
          ],
        },
        { slug: "docs/tiptap" },
      ],
    }),
  ],
});
