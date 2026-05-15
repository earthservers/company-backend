import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import type { ScalarOptions } from '@scalar/docusaurus';

const config: Config = {
  title: 'Stoat Developers',
  tagline: 'Developer documentation for Stoat',
  favicon: 'https://company.earthservers.net/favicon.svg',

  future: {
    v4: true,
  },

  url: 'https://developers.company.earthservers.net',
  baseUrl: '/',

  organizationName: 'stoatchat',
  projectName: 'stoatchat',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          editUrl:
            'https://github.com/stoatchat/stoatchat/tree/main/docs/',
        },
      } satisfies Preset.Options,
    ],
  ],

  plugins: [
    [
      '@scalar/docusaurus',
      {
        label: 'API Reference',
        route: '/api-reference',
        showNavLink: true,
        configuration: {
          url: 'https://company.earthservers.net/api/openapi.json',
        },
      } as ScalarOptions,
    ],
    [
      '@docusaurus/plugin-client-redirects',
      {
        fromExtensions: ['html', 'htm'],
        redirects: [
          // legacy docs website (stoatchat/developer-wiki)
          {
            from: '/developers/api/reference.html',
            to: '/api-reference',
          },
          {
            from: '/contrib.html',
            to: '/developing/contrib',
          },
          {
            from: '/contrib',
            to: '/developing/contrib',
          },
        ],
      }
    ],
  ],

  themeConfig: {
    // image: 'img/docusaurus-social-card.jpg',
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Stoat Developers',
      logo: {
        alt: 'Stoat',
        src: 'https://company.earthservers.net/favicon.svg',
      },
      items: [
        {
          type: 'doc',
          docId: 'index',
          label: 'Docs'
        },
        {
          href: 'https://github.com/stoatchat',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Developers',
          items: [
            {
              label: 'Source Code',
              href: 'https://github.com/stoatchat'
            },
            {
              label: 'Help Translate',
              href: 'https://translate.company.earthservers.net'
            },
          ],
        },
        {
          title: 'Team',
          items: [
            {
              label: 'About',
              href: 'https://company.earthservers.net/about'
            },
            {
              label: 'Blog and Changelogs',
              href: 'https://company.earthservers.net/updates'
            },
            {
              label: 'Contact',
              href: 'https://support.company.earthservers.net'
            },
          ],
        },
        {
          title: 'Stoat on Socials',
          items: [
            {
              label: 'Bluesky',
              href: 'https://bsky.app/profile/company.earthservers.net'
            },
            {
              label: 'Reddit',
              href: 'https://reddit.com/r/stoatchat'
            },
            {
              label: 'Stoat Server',
              href: 'https://company.earthservers.net/invite/Testers'
            },
          ],
        },
        {
          title: 'Legal',
          items: [
            {
              label: 'Community Guidelines',
              href: 'https://company.earthservers.net/legal/community-guidelines'
            },
            {
              label: 'Terms of Service',
              href: 'https://company.earthservers.net/legal/terms'
            },
            {
              label: 'Privacy Policy',
              href: 'https://company.earthservers.net/legal/privacy'
            },
            {
              label: 'Imprint',
              href: 'https://company.earthservers.net/legal/imprint'
            },
          ],
        },
      ],
      copyright: `© Revolt Platforms Ltd, ${new Date().getFullYear()}`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
