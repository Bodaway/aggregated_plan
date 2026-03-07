import React from 'react';
import ReactDOM from 'react-dom/client';
import { Provider } from 'urql';
import { urqlClient } from '@/lib/urql-client';
import { App } from '@/App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <Provider value={urqlClient}>
      <App />
    </Provider>
  </React.StrictMode>
);
