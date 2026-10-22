/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Origin of the AegisCMS server when Admin Studio is hosted elsewhere (see api/client.ts) */
  readonly VITE_AEGIS_SERVER_ORIGIN?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
