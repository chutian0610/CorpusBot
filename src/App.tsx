import { useState } from 'react';
import { BookOpenText, Files, MessagesSquare, Search, Settings } from 'lucide-react';

type ViewId = 'wiki' | 'files' | 'chat' | 'search' | 'settings';

const navigation = [
  { id: 'wiki', label: 'Wiki', icon: BookOpenText },
  { id: 'files', label: 'Files', icon: Files },
  { id: 'chat', label: 'Chat', icon: MessagesSquare },
  { id: 'search', label: 'Search', icon: Search },
  { id: 'settings', label: 'Settings', icon: Settings },
] as const;

export default function App() {
  const [activeView, setActiveView] = useState<ViewId>('wiki');

  return (
    <main className="flex h-screen min-h-0 flex-col bg-paper text-ink lg:grid lg:grid-cols-[4rem_18rem_minmax(0,1fr)]">
      <nav
        aria-label="Primary"
        className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-stone-200 bg-white px-3 py-3 lg:min-h-0 lg:flex-col lg:overflow-visible lg:border-b-0 lg:border-r lg:py-4"
      >
        {navigation.map(({ id, label, icon: Icon }) => {
          const selected = activeView === id;
          return (
            <button
              key={id}
              type="button"
              aria-label={label}
              aria-current={selected ? 'page' : undefined}
              title={label}
              onClick={() => setActiveView(id)}
              className={`flex size-11 items-center justify-center rounded-md border transition ${
                selected
                  ? 'border-moss/30 bg-moss/10 text-moss'
                  : 'border-transparent text-stone-500 hover:bg-stone-100 hover:text-stone-800'
              }`}
            >
              <Icon className="size-5" aria-hidden="true" />
            </button>
          );
        })}
      </nav>

      <aside className="hidden min-h-0 min-w-0 flex-col border-r border-stone-200 bg-white lg:flex">
        <header className="flex h-12 shrink-0 items-center border-b border-stone-200 px-4">
          <h1 className="text-sm font-semibold tracking-normal">Wiki</h1>
        </header>
        <div className="flex min-h-0 flex-1 items-center justify-center p-6">
          <p className="text-sm text-stone-500">No pages yet</p>
        </div>
      </aside>

      <section className="grid min-h-0 min-w-0 flex-1 grid-rows-[minmax(0,1fr)_minmax(0,1fr)] lg:grid-cols-2 lg:grid-rows-1">
        <div className="flex min-h-0 min-w-0 flex-col border-r border-stone-200 bg-white">
          <header className="flex h-12 shrink-0 items-center border-b border-stone-200 px-4">
            <h2 className="text-sm font-semibold tracking-normal">Chat</h2>
          </header>
          <div className="flex min-h-0 flex-1 items-center justify-center p-6">
            <p className="text-sm text-stone-500">No conversation</p>
          </div>
        </div>

        <div className="flex min-h-0 min-w-0 flex-col bg-paper">
          <header className="flex h-12 shrink-0 items-center border-b border-stone-200 bg-white px-4">
            <h2 className="text-sm font-semibold tracking-normal">Preview</h2>
          </header>
          <div className="flex min-h-0 flex-1 items-center justify-center p-6">
            <p className="text-sm text-stone-500">No page selected</p>
          </div>
        </div>
      </section>
    </main>
  );
}
