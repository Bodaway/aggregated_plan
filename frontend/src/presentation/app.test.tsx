import { render, screen } from '@testing-library/react';
import { App } from './app';

describe('App', () => {
  it('should render the application', () => {
    render(<App />);
    expect(screen.getByText('Aggregated Plan')).toBeInTheDocument();
  });
});
