/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Base URL of the AegisCMS server when Admin Studio is hosted elsewhere (see api/client.ts) */
  readonly VITE_AEGIS_SERVER_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
