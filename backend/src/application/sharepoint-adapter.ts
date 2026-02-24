export type SharePointFileContent = {
  readonly buffer: Buffer;
  readonly fileName: string;
};

export type SharePointAdapter = {
  readonly downloadFile: (graphToken: string) => Promise<SharePointFileContent>;
};
