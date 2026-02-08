// Copyright The Glide Authors
// SPDX-License-Identifier: MIT OR Apache-2.0

import { fromMarkdown } from "mdast-util-from-markdown";
import { directive } from "micromark-extension-directive";
import { directiveFromMarkdown } from "mdast-util-directive";

interface PluginOptions {
  githubUrl: string;
}

/**
 * Remark plugin that replaces the content of getting-started.md with filtered README content
 */
export function remarkReadmeImport(options: PluginOptions) {
  const githubUrl = options.githubUrl;

  return function transformer(tree: any, file: any) {
    // Only process the README.md file
    if (!file.path || !file.path.includes("README.md")) {
      return;
    }

    try {
      const readmeContent = file.value;
      const filteredContent = filterReadmeContent(readmeContent);
      const withAdmonitions = convertAdmonitions(filteredContent);
      const processedContent = processLinksForGithub(
        withAdmonitions,
        githubUrl,
      );

      // Parse the processed markdown back into AST nodes
      // Include directive support so that :::note etc. are parsed correctly
      const readmeAst = fromMarkdown(processedContent, {
        extensions: [directive()],
        mdastExtensions: [directiveFromMarkdown()],
      });

      // Replace all content after the frontmatter with README content
      // Keep the root node but replace its children
      tree.children = readmeAst.children;
    } catch (error) {
      console.error("Error processing README import:", error);

      // Replace with error message
      tree.children = [
        {
          type: "paragraph",
          children: [
            {
              type: "text",
              value: `Error: Could not import README content - ${error}`,
            },
          ],
        },
      ];
    }
  };
}

/**
 * Filters README content by removing sections marked with GUIDE_EXCLUDE comments
 */
function filterReadmeContent(content: string): string {
  const lines = content.split("\n");
  const filteredLines: string[] = [];
  let isExcluding = false;

  for (const line of lines) {
    const trimmedLine = line.trim();

    if (trimmedLine === "<!-- GUIDE_EXCLUDE_START -->") {
      isExcluding = true;
      continue;
    }

    if (trimmedLine === "<!-- GUIDE_EXCLUDE_END -->") {
      isExcluding = false;
      continue;
    }

    if (!isExcluding) {
      filteredLines.push(line);
    }
  }

  return filteredLines.join("\n").trim();
}

/**
 * Converts GitHub-style admonitions to Starlight format
 */
function convertAdmonitions(content: string): string {
  const lines = content.split("\n");
  const result: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    const match = line.match(/^>\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/i);

    if (match) {
      const admonitionType = match[1].toLowerCase();
      // Map GitHub admonition types to Starlight types
      const typeMap: Record<string, string> = {
        note: "note",
        tip: "tip",
        important: "note",
        warning: "caution",
        caution: "caution",
      };
      const starlightType = typeMap[admonitionType] || admonitionType;

      // Collect the content of the admonition (lines starting with >)
      const admonitionLines: string[] = [];
      i++; // Skip the admonition header line

      while (i < lines.length && lines[i].startsWith(">")) {
        // Remove the leading > and optional space
        const contentLine = lines[i].replace(/^>\s?/, "");
        admonitionLines.push(contentLine);
        i++;
      }

      // Convert to Starlight format
      result.push(`:::${starlightType}`);
      result.push(...admonitionLines);
      result.push(":::");
    } else {
      result.push(line);
      i++;
    }
  }

  return result.join("\n");
}

/**
 * Processes relative links to point to GitHub repository
 */
function processLinksForGithub(content: string, githubUrl: string): string {
  let processed = content;

  // Convert relative links with ./ to absolute GitHub links
  processed = processed.replace(
    /\[([^\]]+)\]\(\.\/([^)]+)\)/g,
    `[$1](${githubUrl}/blob/main/$2)`,
  );

  // Convert relative links without ./ prefix to GitHub links (but not already processed ones)
  processed = processed.replace(
    /\[([^\]]+)\]\((?!https?:\/\/)([^)]+\.(toml|md|rs|txt|json|yaml|yml))\)/g,
    `[$1](${githubUrl}/blob/main/$2)`,
  );

  return processed;
}

export default remarkReadmeImport;
