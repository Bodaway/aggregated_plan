import React from 'react';
import ReactDOM from 'react-dom/client';
import { Provider } from 'urql';
import { urqlClient } from '@/lib/urql-client';
import { App } from '@/App';
import { AuthGate } from '@/components/auth/AuthGate';
import '@/styles/cybernord.css';
import './index.css';
import '@/styles/app-shell.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Provider value={urqlClient}>
      <AuthGate>
        <App />
      </AuthGate>
    </Provider>
  </React.StrictMode>
);
