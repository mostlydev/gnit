import { defineConfig } from 'vitepress'

const base = process.env.VITEPRESS_BASE || '/'

export default defineConfig({
  title: 'Nit',
  description: 'Git-native multi-repo workspaces',
  base,
  cleanUrls: true,
  head: [
    ['meta', { name: 'theme-color', content: '#1d6f5f' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:site_name', content: 'Nit' }],
    ['meta', { property: 'og:title', content: 'Nit - Git-native multi-repo workspaces' }],
    ['meta', { property: 'og:description', content: 'A small Git-native layer for changes, pins, checkout, and review across independent repositories.' }],
  ],
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/quickstart' },
      { text: 'Design', link: '/guide/design' },
      { text: 'CLI', link: '/guide/cli' },
    ],
    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Quickstart', link: '/guide/quickstart' },
          { text: 'Concepts', link: '/guide/concepts' },
          { text: 'CLI', link: '/guide/cli' },
          { text: 'Design', link: '/guide/design' },
        ],
      },
    ],
    search: {
      provider: 'local',
    },
    footer: {
      message: 'Nit is an early design for Git-native multi-repo workspaces.',
    },
  },
})

