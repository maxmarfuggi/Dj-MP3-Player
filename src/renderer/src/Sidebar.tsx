import { useState } from 'react';
import type { Library, LibrarySource, PlaylistNode } from '../../shared/types';
import { ChevronIcon, DiscIcon, FolderIcon, PlaylistIcon, PlusIcon, UsbIcon } from './icons';

export interface Selection {
  libraryId: string;
  /** null selects all tracks of the library. */
  playlistId: string | null;
}

interface SidebarProps {
  libraries: Library[];
  selection: Selection | null;
  onSelect(selection: Selection): void;
  onOpenFolder(): void;
}

export const SOURCE_LABELS: Record<LibrarySource, string> = { rekordbox: 'rekordbox', serato: 'Serato' };
const INDENT_PX = 14;

export function Sidebar({ libraries, selection, onSelect, onOpenFolder }: SidebarProps) {
  // Keys are `<libraryId>/<playlistId>`.
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  const toggle = (key: string) => {
    setExpanded((previous) => {
      const next = new Set(previous);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  };

  const renderNodes = (library: Library, nodes: PlaylistNode[], depth: number) =>
    nodes.map((node) => {
      const key = `${library.id}/${node.id}`;
      const hasChildren = node.children.length > 0;
      const isExpanded = expanded.has(key);
      const isSelected = selection?.libraryId === library.id && selection.playlistId === node.id;
      return (
        <li key={node.id}>
          <div
            className={`tree-row${isSelected ? ' selected' : ''}`}
            style={{ paddingLeft: 4 + depth * INDENT_PX }}
          >
            {hasChildren ? (
              <button
                className={`tree-toggle${isExpanded ? ' expanded' : ''}`}
                aria-label={isExpanded ? 'Collapse' : 'Expand'}
                onClick={() => toggle(key)}
              >
                <ChevronIcon size={12} />
              </button>
            ) : (
              <span className="tree-toggle" />
            )}
            <button
              className="tree-label"
              title={node.name}
              // Folders hold no tracks of their own, so clicking one just opens it.
              onClick={() =>
                node.kind === 'folder'
                  ? toggle(key)
                  : onSelect({ libraryId: library.id, playlistId: node.id })
              }
            >
              {node.kind === 'folder' ? <FolderIcon size={15} /> : <PlaylistIcon size={15} />}
              <span className="tree-name">{node.name}</span>
              {node.kind === 'playlist' && <span className="tree-count">{node.trackIds.length}</span>}
            </button>
          </div>
          {hasChildren && isExpanded && <ul>{renderNodes(library, node.children, depth + 1)}</ul>}
        </li>
      );
    });

  return (
    <aside className="sidebar">
      {/* On macOS the window buttons sit here, and it is where the window is dragged from. */}
      <div className="sidebar-top" />
      <nav className="sidebar-scroll">
        <h2 className="sidebar-heading">Libraries</h2>
        {libraries.length === 0 && <p className="sidebar-empty">No DJ USB connected</p>}
        {libraries.map((library) => {
          const allSelected = selection?.libraryId === library.id && selection.playlistId === null;
          return (
            <section key={library.id} className={`library source-${library.source}`}>
              <h3 className="library-title" title={library.volumePath}>
                <span className="library-icon">
                  <UsbIcon size={14} />
                </span>
                <span className="library-name">{library.volumeName}</span>
                <span className="badge">{SOURCE_LABELS[library.source]}</span>
              </h3>
              <ul>
                <li>
                  <div className={`tree-row${allSelected ? ' selected' : ''}`} style={{ paddingLeft: 4 }}>
                    <span className="tree-toggle" />
                    <button
                      className="tree-label"
                      onClick={() => onSelect({ libraryId: library.id, playlistId: null })}
                    >
                      <DiscIcon size={15} />
                      <span className="tree-name">All tracks</span>
                      <span className="tree-count">{library.tracks.length}</span>
                    </button>
                  </div>
                </li>
                {renderNodes(library, library.playlists, 0)}
              </ul>
            </section>
          );
        })}
      </nav>
      <div className="sidebar-footer">
        <button className="open-folder" onClick={onOpenFolder}>
          <PlusIcon size={14} />
          Open folder…
        </button>
      </div>
    </aside>
  );
}
