import '../dist/styles/common-shared.css';

/** @type { import('@storybook/web-components-vite').Preview } */
const preview = {
  parameters: {
    actions: { argTypesRegex: '^on[A-Z].*' },
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/,
      },
    },
    docs: {
      extractComponentDescription: (component, { notes }) => {
        if (notes) {
          return typeof notes === 'string' ? notes : notes.markdown || notes.text;
        }
        return null;
      },
    },
  },
  // Global decorators for all stories
  decorators: [
    (story) => {
      // Import global styles if needed
      const link = document.createElement('link');
      link.rel = 'stylesheet';
      link.href = '/styles/global.css'; // Adjust path as needed
      
      if (!document.head.querySelector(`link[href="/styles/global.css"]`)) {
        document.head.appendChild(link);
      }
      
      return story();
    },
  ],
};

export default preview;