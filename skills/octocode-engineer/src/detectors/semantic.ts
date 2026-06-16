import path from 'node:path';

import * as ts from 'typescript';

import type { SemanticContext, SemanticProfile } from '../analysis/semantic.js';
import type { Finding } from '../types/index.js';

type FindingDraft = Omit<Finding, 'id'>;

export function detectSemanticDeadExports(
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const [name, info] of profile.referenceCountByExport) {
      if (info.count === 0) {
        findings.push({
          severity: 'high',
          category: 'semantic-dead-export',
          file: profile.file,
          lineStart: info.lineStart,
          lineEnd: info.lineEnd,
          title: `Semantically dead export: ${name}`,
          reason: `Exported symbol "${name}" has zero semantic references across the entire program (confirmed via TypeChecker, not just import matching).`,
          files: [profile.file],
          suggestedFix: {
            strategy:
              'Remove the export or delete the symbol if unused internally.',
            steps: [
              'Verify the symbol is not used via dynamic imports or runtime reflection.',
              'Remove the export keyword, or delete the symbol entirely if also unused locally.',
              'Re-run scan to confirm finding is resolved.',
            ],
          },
          impact:
            'Dead exports bloat the public API surface and confuse contributors.',
          tags: ['architecture', 'dead-code', 'semantic'],
          lspHints: [
            {
              tool: 'lspGetSemantics', semanticType: 'references',
              symbolName: name,
              lineHint: info.lineStart,
              file: profile.file,
              expectedResult: 'zero references confirms dead export',
            },
          ],
        });
      }
    }
  }

  return findings;
}

export function detectOverAbstraction(
  ctx: SemanticContext,
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  const interfaceImplCounts = new Map<
    string,
    { files: Set<string>; line: number; file: string }
  >();

  for (const profile of profiles) {
    const sourceFile = ctx.program.getSourceFile(
      path.resolve(ctx.root, profile.file)
    );
    if (!sourceFile) continue;

    const visit = (node: ts.Node): void => {
      if (ts.isInterfaceDeclaration(node) && node.name) {
        const name = node.name.text;
        const line =
          sourceFile!.getLineAndCharacterOfPosition(node.getStart(sourceFile!))
            .line + 1;

        const impls = ctx.service.getImplementationAtPosition(
          path.resolve(ctx.root, profile.file),
          node.name.getStart(sourceFile!)
        );

        const implFiles = new Set<string>();
        if (impls) {
          for (const impl of impls) {
            const implFile = impl.fileName;
            if (
              implFile !== path.resolve(ctx.root, profile.file) ||
              impl.textSpan.start !== node.getStart(sourceFile!)
            ) {
              implFiles.add(implFile);
            }
          }
        }

        if (!interfaceImplCounts.has(name)) {
          interfaceImplCounts.set(name, {
            files: new Set(),
            line,
            file: profile.file,
          });
        }
        const entry = interfaceImplCounts.get(name)!;
        for (const f of implFiles) entry.files.add(f);
      }
      ts.forEachChild(node, visit);
    };
    ts.forEachChild(sourceFile, visit);
  }

  for (const [name, info] of interfaceImplCounts) {
    if (info.files.size === 1) {
      const implFile = [...info.files][0];
      const relImpl = path.relative(ctx.root, implFile);
      findings.push({
        severity: 'medium',
        category: 'over-abstraction',
        file: info.file,
        lineStart: info.line,
        lineEnd: info.line,
        title: `Over-abstraction: interface ${name} has exactly 1 implementor`,
        reason: `Interface "${name}" is implemented only by one class in "${relImpl}". The abstraction layer adds complexity without enabling polymorphism.`,
        files: [info.file, relImpl],
        suggestedFix: {
          strategy:
            'Inline the interface into the concrete class or keep it only if future implementors are planned.',
          steps: [
            'Evaluate whether the interface is needed for testing (mocking) or future extensibility.',
            'If not, merge the interface declaration into the concrete class.',
            'Update consumers to depend on the concrete class directly.',
          ],
        },
        impact:
          'Over-abstraction adds indirection without polymorphic benefit, increasing cognitive load.',
        tags: ['architecture', 'abstraction', 'semantic'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'references',
            symbolName: name,
            lineHint: info.line,
            file: info.file,
            expectedResult:
              'exactly 1 implementation confirms over-abstraction',
          },
        ],
      });
    }
  }

  return findings;
}

export function detectConcreteDependency(
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const imp of profile.concreteImports) {
      findings.push({
        severity: 'medium',
        category: 'concrete-dependency',
        file: profile.file,
        lineStart: imp.lineStart,
        lineEnd: imp.lineStart,
        title: `Concrete dependency: ${profile.file} imports class ${imp.name}`,
        reason: `Module imports concrete class "${imp.name}" from "${imp.targetFile}" instead of an interface or abstract class. This violates the Dependency Inversion Principle (DIP).`,
        files: [profile.file, imp.targetFile],
        suggestedFix: {
          strategy:
            'Depend on an interface or abstract class instead of the concrete implementation.',
          steps: [
            'Extract an interface from the concrete class covering the methods used by this module.',
            'Update imports to reference the interface instead of the concrete class.',
            'Use dependency injection to provide the concrete implementation at runtime.',
          ],
        },
        impact:
          'Concrete dependencies make modules harder to test and tightly coupled to implementation details.',
        tags: ['architecture', 'dip', 'coupling', 'semantic'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'definition',
            symbolName: imp.name,
            lineHint: imp.lineStart,
            file: profile.file,
            expectedResult:
              'resolves to concrete class (not interface/abstract)',
          },
        ],
      });
    }
  }

  return findings;
}

export function detectCircularTypeDependency(
  ctx: SemanticContext,
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  const typeGraph = new Map<string, Set<string>>();

  for (const profile of profiles) {
    const sourceFile = ctx.program.getSourceFile(
      path.resolve(ctx.root, profile.file)
    );
    if (!sourceFile) continue;

    const fileTypes = new Set<string>();
    const fileTypeRefs = new Map<string, Set<string>>();

    const visit = (node: ts.Node): void => {
      if (
        (ts.isInterfaceDeclaration(node) || ts.isTypeAliasDeclaration(node)) &&
        node.name
      ) {
        const typeName = `${profile.file}::${node.name.text}`;
        fileTypes.add(typeName);
        const refs = new Set<string>();

        const collectRefs = (child: ts.Node): void => {
          if (
            ts.isTypeReferenceNode(child) &&
            ts.isIdentifier(child.typeName)
          ) {
            const refName = child.typeName.text;
            const sym = ctx.checker.getSymbolAtLocation(child.typeName);
            if (sym) {
              const decl = sym.getDeclarations?.()?.[0];
              if (decl) {
                const declFile = decl.getSourceFile().fileName;
                const relFile = path.relative(ctx.root, declFile);
                refs.add(`${relFile}::${refName}`);
              }
            }
          }
          ts.forEachChild(child, collectRefs);
        };
        ts.forEachChild(node, collectRefs);
        fileTypeRefs.set(typeName, refs);
      }
      ts.forEachChild(node, visit);
    };
    ts.forEachChild(sourceFile, visit);

    for (const [typeName, refs] of fileTypeRefs) {
      if (!typeGraph.has(typeName)) typeGraph.set(typeName, new Set());
      for (const ref of refs) typeGraph.get(typeName)!.add(ref);
    }
  }

  const visited = new Set<string>();
  const inStack = new Set<string>();
  const reportedCycles = new Set<string>();

  const dfs = (node: string, stackPath: string[]): void => {
    if (inStack.has(node)) {
      const cycleStart = stackPath.indexOf(node);
      if (cycleStart >= 0) {
        const cycle = stackPath.slice(cycleStart);
        const key = [...cycle].sort().join('→');
        if (!reportedCycles.has(key) && cycle.length >= 2) {
          reportedCycles.add(key);
          const first = cycle[0];
          const [file] = first.split('::');
          findings.push({
            severity: 'high',
            category: 'circular-type-dependency',
            file,
            lineStart: 1,
            lineEnd: 1,
            title: `Circular type dependency: ${cycle.map(c => c.split('::')[1]).join(' → ')}`,
            reason: `Type-level circular dependency detected: ${cycle.map(c => c.split('::')[1]).join(' → ')} → ${cycle[0].split('::')[1]}. Types reference each other creating a cycle.`,
            files: [...new Set(cycle.map(c => c.split('::')[0]))],
            suggestedFix: {
              strategy:
                'Break the type cycle by extracting shared type definitions.',
              steps: [
                'Identify the minimal set of type properties causing the cycle.',
                'Extract shared types to a dedicated types file that both sides can import.',
                'Replace direct type references with the shared type.',
              ],
            },
            impact:
              'Circular type dependencies make types harder to understand, refactor, and can cause issues with type inference.',
            tags: ['architecture', 'types', 'cycle', 'semantic'],
          });
        }
      }
      return;
    }
    if (visited.has(node)) return;

    inStack.add(node);
    stackPath.push(node);

    for (const neighbor of typeGraph.get(node) ?? []) {
      dfs(neighbor, stackPath);
    }

    stackPath.pop();
    inStack.delete(node);
    visited.add(node);
  };

  for (const node of typeGraph.keys()) {
    dfs(node, []);
  }

  return findings;
}

export function detectUnusedParameters(
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const param of profile.unusedParams) {
      findings.push({
        severity: 'medium',
        category: 'unused-parameter',
        file: profile.file,
        lineStart: param.lineStart,
        lineEnd: param.lineEnd,
        title: `Unused parameter: ${param.paramName} in ${param.functionName}`,
        reason: `Parameter "${param.paramName}" in function "${param.functionName}" is never referenced in the function body (confirmed via semantic analysis).`,
        files: [profile.file],
        suggestedFix: {
          strategy:
            'Remove the parameter or prefix with underscore to indicate intentional non-use.',
          steps: [
            'Check if the parameter is required by an interface or callback signature.',
            'If not required, remove it and update all call sites.',
            'If required by contract, prefix with _ (e.g. _unused) to signal intent.',
          ],
        },
        impact:
          'Unused parameters add noise to function signatures and confuse callers about what the function actually needs.',
        tags: ['code-quality', 'parameters', 'semantic'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'references',
            symbolName: param.paramName,
            lineHint: param.lineStart,
            file: profile.file,
            expectedResult: 'zero non-declaration references confirms unused',
          },
        ],
      });
    }
  }

  return findings;
}

export function detectDeepOverrideChain(
  profiles: SemanticProfile[],
  threshold: number = 3
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const chain of profile.overrideChains) {
      if (chain.depth > threshold) {
        findings.push({
          severity: chain.depth > 4 ? 'high' : 'medium',
          category: 'deep-override-chain',
          file: profile.file,
          lineStart: chain.lineStart,
          lineEnd: chain.lineStart,
          title: `Deep override chain: ${chain.className}.${chain.methodName} (depth ${chain.depth})`,
          reason: `Method "${chain.methodName}" in class "${chain.className}" overrides a method ${chain.depth} levels up in the inheritance chain (threshold: ${threshold}).`,
          files: [profile.file],
          suggestedFix: {
            strategy:
              'Reduce override depth by flattening the class hierarchy or using the template method pattern.',
            steps: [
              'Identify if intermediate overrides are necessary or if they just pass through.',
              'Consider extracting the behavior into a strategy or template method.',
              'Flatten unnecessary intermediate classes.',
            ],
          },
          impact:
            'Deep override chains make method behavior unpredictable — understanding what runs requires tracing through many classes.',
          tags: ['code-quality', 'inheritance', 'override', 'semantic'],
        });
      }
    }
  }

  return findings;
}

export function detectInterfaceCompliance(
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const impl of profile.interfaceImpls) {
      const issues: string[] = [];
      if (impl.missingMembers.length > 0) {
        issues.push(`missing members: ${impl.missingMembers.join(', ')}`);
      }
      if (impl.anycastMembers.length > 0) {
        issues.push(`any-cast members: ${impl.anycastMembers.join(', ')}`);
      }

      if (issues.length > 0) {
        findings.push({
          severity: impl.missingMembers.length > 0 ? 'high' : 'medium',
          category: 'interface-compliance',
          file: impl.classFile,
          lineStart: impl.classLine,
          lineEnd: impl.classLine,
          title: `Fragile interface compliance: ${impl.className} implements ${impl.interfaceName}`,
          reason: `Class "${impl.className}" implements "${impl.interfaceName}" with issues: ${issues.join('; ')}.`,
          files: [impl.classFile],
          suggestedFix: {
            strategy:
              'Fix the implementation to fully satisfy the interface contract.',
            steps: [
              ...(impl.missingMembers.length > 0
                ? [
                    `Implement missing members: ${impl.missingMembers.join(', ')}.`,
                  ]
                : []),
              ...(impl.anycastMembers.length > 0
                ? [
                    `Replace \`any\` types with proper types for: ${impl.anycastMembers.join(', ')}.`,
                  ]
                : []),
              'Enable strict type checking to catch these at compile time.',
            ],
          },
          impact:
            'Incomplete interface implementations create runtime surprises and defeat the purpose of type contracts.',
          tags: ['code-quality', 'types', 'interface', 'semantic'],
          lspHints: [
            {
              tool: 'lspGetSemantics', semanticType: 'definition',
              symbolName: impl.interfaceName,
              lineHint: impl.classLine,
              file: impl.classFile,
              expectedResult: 'interface definition showing expected contract',
            },
          ],
        });
      }
    }
  }

  return findings;
}

export function detectUnusedImports(
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const imp of profile.unusedImports) {
      findings.push({
        severity: 'low',
        category: 'unused-import',
        file: profile.file,
        lineStart: imp.lineStart,
        lineEnd: imp.lineStart,
        title: `Unused import: ${imp.name}`,
        reason: `Imported symbol "${imp.name}" is never referenced in this file (confirmed via semantic analysis, not just text matching).`,
        files: [profile.file],
        suggestedFix: {
          strategy: 'Remove the unused import statement.',
          steps: [
            'Verify the import is not used for side effects (e.g. polyfills, CSS).',
            'Remove the import statement.',
            'If part of a multi-import, remove only the unused symbol.',
          ],
        },
        impact:
          'Unused imports slow down IDE performance, increase bundle size (if not tree-shaken), and add noise.',
        tags: ['dead-code', 'imports', 'semantic'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'references',
            symbolName: imp.name,
            lineHint: imp.lineStart,
            file: profile.file,
            expectedResult: 'zero usage references confirms unused import',
          },
        ],
      });
    }
  }

  return findings;
}

export function detectOrphanImplementation(
  ctx: SemanticContext,
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    const sourceFile = ctx.program.getSourceFile(
      path.resolve(ctx.root, profile.file)
    );
    if (!sourceFile) continue;

    const visit = (node: ts.Node): void => {
      if (ts.isClassDeclaration(node) && node.name) {
        const hasHeritage =
          node.heritageClauses && node.heritageClauses.length > 0;
        if (hasHeritage) {
          ts.forEachChild(node, visit);
          return;
        }

        const isExported = node.modifiers?.some(
          m => m.kind === ts.SyntaxKind.ExportKeyword
        );
        if (!isExported) {
          ts.forEachChild(node, visit);
          return;
        }

        const refs = ctx.service.findReferences(
          path.resolve(ctx.root, profile.file),
          node.name.getStart(sourceFile!)
        );

        let externalUsage = 0;
        if (refs) {
          for (const group of refs) {
            for (const ref of group.references) {
              if (!ref.isDefinition) {
                const refFile = ref.fileName;
                if (refFile !== path.resolve(ctx.root, profile.file)) {
                  externalUsage++;
                }
              }
            }
          }
        }

        if (externalUsage === 0) {
          const line =
            sourceFile!.getLineAndCharacterOfPosition(
              node.getStart(sourceFile!)
            ).line + 1;
          findings.push({
            severity: 'medium',
            category: 'orphan-implementation',
            file: profile.file,
            lineStart: line,
            lineEnd: line,
            title: `Orphan implementation: class ${node.name.text}`,
            reason: `Exported class "${node.name.text}" has no external references and does not implement any interface or extend any base class. It may be unreachable dead code.`,
            files: [profile.file],
            suggestedFix: {
              strategy:
                'Verify the class is needed and wire it in, or remove it.',
              steps: [
                'Check if the class is used via dynamic imports, reflection, or DI containers.',
                'If unused, remove the class and its export.',
                'If needed, wire it into the dependency graph via an interface or direct import.',
              ],
            },
            impact:
              'Orphan implementations waste maintenance effort and bloat the codebase.',
            tags: ['dead-code', 'class', 'orphan', 'semantic'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'references',
                symbolName: node.name.text,
                lineHint: line,
                file: profile.file,
                expectedResult: 'zero external references confirms orphan',
              },
            ],
          });
        }
      }
      ts.forEachChild(node, visit);
    };
    ts.forEachChild(sourceFile, visit);
  }

  return findings;
}

export function detectShotgunSurgery(
  profiles: SemanticProfile[],
  threshold: number = 8
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const [name, info] of profile.referenceCountByExport) {
      if (info.uniqueFiles >= threshold) {
        findings.push({
          severity: info.uniqueFiles > 12 ? 'high' : 'medium',
          category: 'shotgun-surgery',
          file: profile.file,
          lineStart: info.lineStart,
          lineEnd: info.lineEnd,
          title: `Shotgun surgery risk: ${name} used in ${info.uniqueFiles} files`,
          reason: `Exported symbol "${name}" is referenced from ${info.uniqueFiles} unique files (threshold: ${threshold}). Any change to this symbol forces coordinated edits across all consumers.`,
          files: [profile.file],
          suggestedFix: {
            strategy:
              'Reduce coupling by introducing a facade, adapter, or event-based decoupling.',
            steps: [
              'Identify the consumers and group them by usage pattern.',
              'Extract a stable interface that consumers depend on instead of the implementation.',
              'Consider the Mediator or Facade pattern to reduce direct dependencies.',
              'If the symbol is a utility, ensure it has a single, well-defined responsibility.',
            ],
          },
          impact:
            'High fan-out symbols are the #1 source of cascading changes during refactoring.',
          tags: ['architecture', 'coupling', 'change-risk', 'semantic'],
          lspHints: [
            {
              tool: 'lspGetSemantics', semanticType: 'references',
              symbolName: name,
              lineHint: info.lineStart,
              file: profile.file,
              expectedResult: `${info.uniqueFiles}+ unique referencing files confirms shotgun surgery risk`,
            },
          ],
        });
      }
    }
  }

  return findings;
}

export function detectMoveToCaller(
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const [name, info] of profile.referenceCountByExport) {
      if (info.uniqueFiles === 1 && info.count > 0) {
        findings.push({
          severity: 'low',
          category: 'move-to-caller',
          file: profile.file,
          lineStart: info.lineStart,
          lineEnd: info.lineEnd,
          title: `Single-consumer export: ${name} (used by 1 file)`,
          reason: `Exported symbol "${name}" is consumed by exactly 1 file. Consider moving it to the consumer or inlining it to reduce module surface.`,
          files: [profile.file],
          suggestedFix: {
            strategy: 'Move the symbol to its only consumer or inline it.',
            steps: [
              'Verify no dynamic or reflection-based usage exists.',
              'Move the function/class/constant to the consumer file.',
              'Remove the export and the import from the consumer.',
              'If the symbol is large, keep it but remove the export keyword.',
            ],
          },
          impact:
            'Single-consumer exports add unnecessary module surface and indirection.',
          tags: ['dead-code', 'module-surface', 'refactoring', 'semantic'],
          lspHints: [
            {
              tool: 'lspGetSemantics', semanticType: 'references',
              symbolName: name,
              lineHint: info.lineStart,
              file: profile.file,
              expectedResult:
                'exactly 1 referencing file confirms single-consumer',
            },
          ],
        });
      }
    }
  }

  return findings;
}

export function detectNarrowableType(
  profiles: SemanticProfile[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const profile of profiles) {
    for (const param of profile.narrowableParams) {
      findings.push({
        severity: 'low',
        category: 'narrowable-type',
        file: profile.file,
        lineStart: param.lineStart,
        lineEnd: param.lineEnd,
        title: `Narrowable param: ${param.functionName}(${param.paramName}: ${param.declaredType}) → ${param.narrowedType}`,
        reason: `Parameter "${param.paramName}" in "${param.functionName}" is declared as \`${param.declaredType}\` but all call sites pass \`${param.narrowedType}\`. The type can be safely narrowed.`,
        files: [profile.file],
        suggestedFix: {
          strategy: 'Narrow the parameter type to match actual usage.',
          steps: [
            `Change the parameter type from \`${param.declaredType}\` to \`${param.narrowedType}\`.`,
            'Verify no future callers need the broader type.',
            'If the function is part of a public API, consider keeping the broad type with a narrower overload.',
          ],
        },
        impact:
          'Overly broad parameter types weaken type checking — narrowing catches bugs at compile time.',
        tags: ['code-quality', 'types', 'refactoring', 'semantic'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'callers',
            symbolName: param.functionName,
            lineHint: param.lineStart,
            file: profile.file,
            expectedResult: `all incoming calls pass ${param.narrowedType}`,
          },
        ],
      });
    }
  }

  return findings;
}

export function runSemanticDetectors(
  ctx: SemanticContext,
  profiles: SemanticProfile[],
  options: { overrideChainThreshold?: number; shotgunThreshold?: number } = {}
): FindingDraft[] {
  const all: FindingDraft[] = [];

  all.push(...detectOverAbstraction(ctx, profiles));
  all.push(...detectConcreteDependency(profiles));
  all.push(...detectCircularTypeDependency(ctx, profiles));
  all.push(...detectUnusedParameters(profiles));
  all.push(
    ...detectDeepOverrideChain(profiles, options.overrideChainThreshold ?? 3)
  );
  all.push(...detectInterfaceCompliance(profiles));
  all.push(...detectUnusedImports(profiles));
  all.push(...detectOrphanImplementation(ctx, profiles));
  all.push(...detectShotgunSurgery(profiles, options.shotgunThreshold ?? 8));
  all.push(...detectMoveToCaller(profiles));
  all.push(...detectNarrowableType(profiles));
  all.push(...detectSemanticDeadExports(profiles));

  return all;
}
