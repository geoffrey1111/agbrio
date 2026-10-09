import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import {PHONE_LAYOUT_REVISION} from './src/features/workbench/interfaceRevision';

export default defineConfig({
  plugins: [react()],
  build: {
    // Publish one versioned public resource graph with the shell. A warm PWA
    // cannot switch to new HTML before all of that version's scripts are cached.
    rollupOptions: { plugins: [{
      name: "aiwr-public-shell-assets",
      generateBundle: {order:"post",handler(_options, bundle) {
        const index=bundle["index.html"];
        if(!index||index.type!=="asset")return;
        const paths=Object.keys(bundle).filter(path=>path.startsWith("assets/")&&/\.(js|css)$/.test(path)).map(path=>`/${path}`);
        const reference=this.emitFile({type:"asset",name:"pwa-shell.json",source:JSON.stringify(paths)});
        const manifest=`/${this.getFileName(reference)}`;
        index.source=String(index.source).replace("</head>",`<meta name="aiwr-shell-assets" content="${manifest}" /><meta name="aiwr-shell-revision" content="${PHONE_LAYOUT_REVISION}" /></head>`);
      }},
    }] },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/runtime/**", "**/browser-profile/**", "**/chrome-profile/**"] },
  },
});
