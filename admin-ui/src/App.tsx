import { useState, useEffect } from 'react';
import { Routes, Route, Navigate, NavLink, useLocation } from 'react-router-dom';
import { LayoutDashboard, Globe, ScrollText, Shield, LogOut } from 'lucide-react';
import { getToken, clearToken } from './lib/api';
import { cn } from './lib/utils';
import Login from './pages/Login';
import Dashboard from './pages/Dashboard';
import Sites from './pages/Sites';
import Logs from './pages/Logs';
import Risk from './pages/Risk';

const NAV = [
  { to: '/', label: '监控', icon: LayoutDashboard },
  { to: '/sites', label: '站点', icon: Globe },
  { to: '/logs', label: '日志', icon: ScrollText },
  { to: '/risk', label: '风控', icon: Shield },
];

function Layout({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  return (
    <div className="min-h-screen flex flex-col">
      <header className="bg-gray-900 text-white px-6 py-3 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="font-semibold text-lg">PoW CAPTCHA</div>
          <span className="badge bg-blue-600 text-white">Admin</span>
        </div>
        <button
          onClick={() => {
            clearToken();
            window.location.reload();
          }}
          className="text-sm text-gray-300 hover:text-white flex items-center gap-1"
        >
          <LogOut size={14} /> 退出
        </button>
      </header>
      <div className="flex flex-1">
        <nav className="w-56 bg-white border-r border-border p-3">
          {NAV.map((item) => {
            const active =
              item.to === '/' ? location.pathname === '/' : location.pathname.startsWith(item.to);
            return (
              <NavLink
                key={item.to}
                to={item.to}
                className={cn(
                  'flex items-center gap-3 px-3 py-2 rounded-md text-sm font-medium mb-1 transition-colors',
                  active
                    ? 'bg-primary text-primary-foreground'
                    : 'text-gray-700 hover:bg-gray-100'
                )}
              >
                <item.icon size={16} />
                {item.label}
              </NavLink>
            );
          })}
        </nav>
        <main className="flex-1 p-6 overflow-auto">{children}</main>
      </div>
    </div>
  );
}

export default function App() {
  const [authed, setAuthed] = useState(!!getToken());
  useEffect(() => {
    const onStorage = () => setAuthed(!!getToken());
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  if (!authed) {
    return <Login onSuccess={() => setAuthed(true)} />;
  }

  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/sites" element={<Sites />} />
        <Route path="/logs" element={<Logs />} />
        <Route path="/risk" element={<Risk />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  );
}
