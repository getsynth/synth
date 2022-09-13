const isTargetVercel = () => {
    return process.env["VERCEL"] === '1'
}

module.exports = {
    title: 'Synth',
    tagline: 'Open-source data generation',
    url: "https://getsynth.com",
    baseUrl: '/docs/',
    onBrokenLinks: 'warn',
    onBrokenMarkdownLinks: 'warn',
    favicon: '/favicon.ico',
    organizationName: 'getsynth', // Usually your GitHub org/user name.
    projectName: 'synth', // Usually your repo name.
    customFields: {
        blogTitle: "Synth - Blog"
    },
    plugins: [
        require('./src/lib/fathom.js'),
        [
            "@papercups-io/docusaurus-plugin",
            {
                accountId: '41ff5b3d-e2c2-42ed-bed3-ef7a6c0dde62',
                title: 'Welcome to Synth',
                subtitle: 'Ask us anything in the chat window below 😊',
                newMessagePlaceholder: 'Start typing...',
                primaryColor: '#00dab8',
                greeting: '',
                requireEmailUpfront: false,
                showAgentAvailability: false,
            },
        ]
    ],
    themeConfig: {
        image: '/img/getsynth_favicon.png',
        fathomAnalytics: {
            siteId: isTargetVercel() ? 'QRVYRJEG' : 'HSFEOKWQ',
        },
        algolia: {
            apiKey: 'b0583a1f7732cee4e8c80f4a86adf57c',
            indexName: 'synth',
        },
        hideableSidebar: true,
        colorMode: {
            defaultMode: 'dark',
            disableSwitch: false,
            respectPrefersColorScheme: false,
        },
        navbar: {
            hideOnScroll: true,
            logo: {
                alt: 'Synth',
                src: '/img/synth_logo_large.png',
                href: 'https://getsynth.com',
                target: '_self'
            },
            items: [
                {
                    to: '/docs/getting_started/synth',
                    activeBasePath: '/docs/getting_started',
                    label: 'Getting Started',
                    position: 'left',
                },
                {
                    to: '/docs/examples/bank',
                    activeBasePath: '/docs/examples',
                    label: 'Examples',
                    position: 'left',
                },
                {
                    to: '/docs/integrations/postgres',
                    activeBasePath: '/docs/integrations',
                    label: 'Integrations',
                    position: 'left',
                },
                {
                    to: '/docs/content/index',
                    activeBasePath: '/docs/content',
                    label: 'Generators',
                    position: 'left',
                },
                {
                    to: 'blog',
                    label: 'Blog',
                    activeBasePath: '/blog',
                    position: 'right'
                },
                {
                    href: 'https://github.com/getsynth/synth',
                    label: 'GitHub',
                    position: 'right',
                },
            ],
        },
        footer: {
            style: 'dark',
            links: [
                {
                    title: 'Learn',
                    items: [
                        {
                            href: '/docs/getting_started/synth',
                            label: 'What is Synth?',
                        },
                        {
                            to: '/docs/getting_started/hello-world',
                            label: 'Getting Started',
                        },
                        {
                            to: '/docs/examples/bank',
                            label: 'Examples',
                        },
                    ],
                },
                {
                    title: 'More',
                    items: [
                        {
                            to: '/docs/content/index',
                            label: 'Generators',
                        },
                        {
                            to: '/docs/integrations/postgres',
                            label: 'Postgres Integration'
                        },
			{
			    to: '/docs/integrations/mysql',
			    label: 'MySQL Integration'
			}
                    ],
                },
                {
                    title: 'Community',
                    items: [
                        {
                            to: '/blog',
                            label: 'Blog',
                        },
                        {
                            href: 'https://github.com/getsynth/synth',
                            label: 'GitHub',
                        }
                    ],
                }
            ],
            logo: {
                alt: 'Built with <3 by OpenQuery in London',
                src: 'img/synth_logo_large.png',
                href: 'https://getsynth.com',
            },
            copyright: `Copyright © ${new Date().getFullYear()} OpenQuery.`,
        },
        announcementBar: {
            id: 'announcementBar', // Increment on change
            content: `⭐️ If you like Synth, <a
                    href="https://github.com/getsynth/synth"
                    rel="noopener noreferrer"
                    target="_blank"
                >give it a star on GitHub!</a>`,
            isCloseable: true
        },
        prism: {
            additionalLanguages: ['rust', 'graphql'],
        },
    },
    presets: [
        [
            '@docusaurus/preset-classic',
            {
                docs: {
                    routeBasePath: '/',
                    sidebarPath: require.resolve('./sidebars.js'),
                    // Please change this to your repo.
                    editUrl:
                        'https://github.com/getsynth/synth/edit/master/docs/',
                },
                blog: {
                    blogSidebarTitle: 'All posts',
                    blogSidebarCount: 'ALL',
                },
                theme: {
                    customCss: require.resolve('./src/css/custom.css')
                },
            },
        ],
    ],
};
