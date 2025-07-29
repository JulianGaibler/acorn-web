import '../dist/styles/common-shared.css';
// import { setCustomElementsManifest } from "@storybook/web-components";
// import { withActions } from "@storybook/addon-actions/decorator";
// import { css, html } from "../dist/dependencies/lit.all.mjs";
// import { MozLitElement } from "../dist/dependencies/lit-utils.mjs";
// import customElementsManifest from "../custom-elements.json";
import { insertFTLIfNeeded, connectFluent } from "./fluent-utils.mjs";

connectFluent();

window.MozXULElement = {
  insertFTLIfNeeded,
};

// Used to set prefs in unprivileged contexts.
window.RPMSetPref = () => {
  /* NOOP */
};
window.RPMGetFormatURLPref = () => {
  /* NOOP */
};

const components = import.meta.glob('../dist/components/moz-*/moz-*.mjs');
Object.keys(components).forEach((path) => {
  if (!path.endsWith('.stories.mjs')) {
    components[path]();
  }
});

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