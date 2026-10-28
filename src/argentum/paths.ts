/** A photo's path, for the interface: its file name, and its virtual copies. Ours. */

/** What follows a photo's path in a virtual copy's: `IMG.CR2?vc=a1b2c3` (Rust: `file_management::VIRTUAL_COPY`). */
export const VIRTUAL_COPY = '?vc=';

/** The file's own name: what follows the last slash or backslash of its path. */
export const fileNameOf = (path: string): string => path.split(/[\\/]/).pop() ?? path;

/** Whether `path` is a virtual copy's rather than a photo's own. */
export const isVirtualCopyPath = (path: string): boolean => path.includes(VIRTUAL_COPY);

/** The photo file on disk behind `path`, a virtual copy's path included. */
export const physicalPathOf = (path: string): string => path.split(VIRTUAL_COPY)[0];
