import { defineConfig } from 'vitepress'

const base = process.env.VITEPRESS_BASE || '/'

export default defineConfig({
  title: 'Gnit',
  description: 'Git-native multi-repo workspaces',
  base,
  cleanUrls: true,
  head: [
    ['meta', { name: 'theme-color', content: '#1d6f5f' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:site_name', content: 'Gnit' }],
    ['meta', { property: 'og:title', content: 'Gnit - Git-native multi-repo workspaces' }],
    ['meta', { property: 'og:description', content: 'A small Git-native layer for changes, pins, checkout, and review across independent repositories.' }],
  ],
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/quickstart' },
      { text: 'Agents', link: '/guide/agents' },
      { text: 'Design', link: '/guide/design' },
      { text: 'CLI', link: '/guide/cli' },
      { text: 'GitHub', link: 'https://github.com/mostlydev/gnit' },
    ],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/mostlydev/gnit' },
    ],
    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Quickstart', link: '/guide/quickstart' },
          { text: 'Concepts', link: '/guide/concepts' },
          { text: 'Agents', link: '/guide/agents' },
          { text: 'CLI', link: '/guide/cli' },
          { text: 'Design', link: '/guide/design' },
          { text: 'Implementation', link: '/guide/implementation' },
        ],
      },
    ],
    search: {
      provider: 'local',
    },
    footer: {
      message: 'Gnit is an early design for Git-native multi-repo workspaces.',
    },
  },
})
