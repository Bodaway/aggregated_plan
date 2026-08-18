import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { Sidebar } from './Sidebar';

describe('Sidebar', () => {
  it('links to the memory tab', () => {
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    );

    expect(screen.getByRole('link', { name: 'Memory' })).toHaveAttribute('href', '/memory');
  });
});
