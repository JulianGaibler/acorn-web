
import path from 'path';

/** @type { import('@storybook/web-components-vite').StorybookConfig } */
const config = {
  stories: ['../dist/components/**/*.stories.mjs'],
  addons: ['@storybook/addon-links', '@storybook/addon-docs'],
  framework: {
    name: '@storybook/web-components-vite',
    options: {},
  },
  viteFinal: (config) => {
    // Ensure Vite can handle your assets and styles
    config.assetsInclude = config.assetsInclude || [];
    config.assetsInclude.push('**/*.svg');
    
    // Handle CSS imports
    config.css = config.css || {};
    config.css.modules = false;
    
    config.resolve.alias = {
        ...config.resolve.alias,
        'lit.all.mjs': path.resolve(__dirname, '../dist/dependencies/lit.all.mjs'),
    };

    return config;
  },
  staticDirs: ['../dist/assets', '../dist/styles'],
};

export default config;