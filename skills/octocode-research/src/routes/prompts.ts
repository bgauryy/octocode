import { Router, type Request, type Response, type NextFunction } from 'express';
import { getMcpContent } from '../mcpCache.js';
import { logPromptCall } from '../index.js';
import { fireAndForgetWithTimeout } from '../utils/asyncTimeout.js';
import { checkReadiness } from '../middleware/readiness.js';

export const promptsRoutes = Router();

promptsRoutes.use(checkReadiness);

declare const __PACKAGE_VERSION__: string;
const PACKAGE_VERSION = __PACKAGE_VERSION__;

interface PromptArg {
  name: string;
  description: string;
  required: boolean;
}

interface PromptInfo {
  name: string;
  description: string;
  arguments?: PromptArg[];
}

promptsRoutes.get('/list', async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();
    
    const prompts: PromptInfo[] = Object.entries(content.prompts).map(([key, prompt]) => ({
      name: key,
      description: prompt.description,
      arguments: prompt.args?.map(arg => ({
        name: arg.name,
        description: arg.description,
        required: arg.required ?? false,
      })),
    }));
    
    res.json({
      success: true,
      data: {
        prompts,
        totalCount: prompts.length,
        version: PACKAGE_VERSION,
      },
      hints: ['Use /prompts/info/{name} to get prompt content'],
    });
  } catch (error) {
    next(error);
  }
});


promptsRoutes.get('/info/:promptName', async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const content = getMcpContent();
    const { promptName } = req.params;
    
    const prompt = content.prompts[promptName];
    
    if (!prompt) {
      const availablePrompts = Object.keys(content.prompts);
      res.status(404).json({
        success: false,
        data: null,
        hints: [
          `Prompt not found: ${promptName}`,
          `Available prompts: ${availablePrompts.slice(0, 5).join(', ')}`,
          'Check spelling and case sensitivity',
          'Use /prompts/list to see all available prompts',
        ],
      });
      return;
    }

    fireAndForgetWithTimeout(
      () => logPromptCall(promptName),
      5000,
      'logPromptCall'
    );

    res.json({
      success: true,
      data: {
        name: prompt.name,
        description: prompt.description,
        arguments: prompt.args?.map(arg => ({
          name: arg.name,
          description: arg.description,
          required: arg.required ?? false,
        })),
        content: prompt.content,
      },
      hints: ['Follow the prompt instructions for best results'],
    });
  } catch (error) {
    next(error);
  }
});
