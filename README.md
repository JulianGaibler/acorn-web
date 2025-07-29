# Acorn Web

Experiment to see if reusable components from Firefox can be extracted and used in a web application.

Core of this projects is a transformation tool written in Rust, that performs the following tasks:

1. **Jar Resolver**: Perfoms a static analysis of Firefox JAR files to map `chrome://` and `resource://` URLs to their corresponding file paths.
2. **Find components**: Finds all moz-* components in Firefox
3. **Find global styles**: Finds all global stylesheets that would be relevant
4. **Find dependencies**: Finds all dependencies of the files found in the previous steps recursively.
5. **Transform files**: Transforms the files to be usable in a web application
6. **Save in dist**: Saves the transformed files in a `dist` directory

## Development

To run the project, you need to have Rust installed. You can install it from [rustup.rs](https://rustup.rs/).

To run the project, use the following command:

```bash
cargo run -- /path/to/firefox/repo ./dist ./config.toml
```

That should generate a `dist` directory with the transformed files.

To run storybook you can use the following command:

```bash
npm install
npm run storybook
```
