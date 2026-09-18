/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly MODE: string;
  readonly VITE_CORPUSBOT_BACKEND?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
