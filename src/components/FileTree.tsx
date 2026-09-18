import { useEffect, useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, FileText, Folder, FolderOpen } from 'lucide-react';

export type FileTreeItem = {
  path: string;
  label: string;
};

export type FileTreeNode = {
  kind: 'folder' | 'file';
  name: string;
  path: string;
  item?: FileTreeItem;
  children: FileTreeNode[];
};

type WorkingNode = FileTreeNode & { children: WorkingNode[] };

function insertPath(root: WorkingNode, item: FileTreeItem) {
  const segments = item.path.split('/').filter(Boolean);
  let current = root;

  for (const [index, segment] of segments.entries()) {
    const isFile = index === segments.length - 1;
    const path = segments.slice(0, index + 1).join('/');
    let next = current.children.find(
      (child) => child.name === segment && child.kind === (isFile ? 'file' : 'folder'),
    );

    if (!next) {
      next = {
        kind: isFile ? 'file' : 'folder',
        name: segment,
        path,
        item: isFile ? item : undefined,
        children: [],
      };
      current.children.push(next);
    }
    current = next;
  }
}

function sortNodes(nodes: WorkingNode[]): WorkingNode[] {
  return nodes
    .map((node) => ({ ...node, children: sortNodes(node.children) }))
    .sort((left, right) => {
      if (left.kind !== right.kind) return left.kind === 'folder' ? -1 : 1;
      return left.name.localeCompare(right.name, undefined, { numeric: true });
    });
}

export function buildFileTree(items: FileTreeItem[]): FileTreeNode[] {
  const root: WorkingNode = { kind: 'folder', name: '', path: '', children: [] };
  for (const item of items) insertPath(root, item);
  return sortNodes(root.children);
}

function FileTreeNodeButton({
  node,
  depth,
  selectedPath,
  expanded,
  onToggleFolder,
  onSelectFile,
}: {
  node: FileTreeNode;
  depth: number;
  selectedPath?: string;
  expanded: Set<string>;
  onToggleFolder: (path: string) => void;
  onSelectFile: (item: FileTreeItem) => void;
}) {
  const isOpen = expanded.has(node.path);
  const selected = node.kind === 'file' && node.path === selectedPath;

  return (
    <li>
      <button
        type="button"
        aria-current={selected ? 'true' : undefined}
        aria-expanded={node.kind === 'folder' ? isOpen : undefined}
        aria-label={node.item?.path ?? node.path}
        title={node.item?.path ?? node.path}
        data-testid={`file-tree:${node.item?.path ?? node.path}`}
        className={`flex w-full items-center gap-1.5 rounded px-2 py-1.5 text-left text-sm transition-colors ${
          selected ? 'bg-moss/10 text-moss' : 'text-stone-700 hover:bg-stone-100'
        }`}
        style={{ paddingLeft: `${depth * 0.75 + 0.5}rem` }}
        onClick={() => {
          if (node.kind === 'folder') onToggleFolder(node.path);
          else if (node.item) onSelectFile(node.item);
        }}
      >
        {node.kind === 'folder' ? (
          <>
            {isOpen ? (
              <ChevronDown className="size-3.5 shrink-0" />
            ) : (
              <ChevronRight className="size-3.5 shrink-0" />
            )}
            {isOpen ? (
              <FolderOpen className="size-4 shrink-0 text-stone-500" />
            ) : (
              <Folder className="size-4 shrink-0 text-stone-500" />
            )}
          </>
        ) : (
          <>
            <span className="w-3.5 shrink-0" />
            <FileText className="size-4 shrink-0 text-stone-500" />
          </>
        )}
        <span className="min-w-0 truncate" title={node.item?.label ?? node.name}>
          {node.item?.label ?? node.name}
        </span>
      </button>
      {node.kind === 'folder' && isOpen && node.children.length > 0 ? (
        <ul className="min-w-0">
          {node.children.map((child) => (
            <FileTreeNodeButton
              key={child.path}
              node={child}
              depth={depth + 1}
              selectedPath={selectedPath}
              expanded={expanded}
              onToggleFolder={onToggleFolder}
              onSelectFile={onSelectFile}
            />
          ))}
        </ul>
      ) : null}
    </li>
  );
}

export function FileTree({
  items,
  selectedPath,
  onSelectFile,
}: {
  items: FileTreeItem[];
  selectedPath?: string;
  onSelectFile: (item: FileTreeItem) => void;
}) {
  const nodes = useMemo(() => buildFileTree(items), [items]);
  const folderKey = useMemo(
    () =>
      nodes
        .map((node) => node.path)
        .sort()
        .join('\n'),
    [nodes],
  );
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    const paths = new Set<string>();
    const visit = (node: FileTreeNode) => {
      if (node.kind === 'folder') {
        paths.add(node.path);
        node.children.forEach(visit);
      }
    };
    nodes.forEach(visit);
    setExpanded(paths);
  }, [folderKey]);

  const toggleFolder = (path: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  return (
    <ul className="min-w-0 space-y-0.5">
      {nodes.map((node) => (
        <FileTreeNodeButton
          key={node.path}
          node={node}
          depth={0}
          selectedPath={selectedPath}
          expanded={expanded}
          onToggleFolder={toggleFolder}
          onSelectFile={onSelectFile}
        />
      ))}
    </ul>
  );
}
