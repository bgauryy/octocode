import * as fsPromises from 'fs/promises';

export async function readJsonFile<T>(
  filePath: string
): Promise<T | undefined> {
  try {
    const content = await fsPromises.readFile(filePath, 'utf-8');
    return JSON.parse(content) as T;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw new Error(`Cannot read configuration at ${filePath}`, {
      cause: error,
    });
  }
}
