import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ApiProblem } from '../api/client';
import { StatusMessage } from './StatusMessage';

describe('StatusMessage', () => {
  it('renders not-found and forbidden failures as the same neutral disclosure', () => {
    const { rerender } = render(<StatusMessage state={new ApiProblem({
      type: 'urn:part-crm:problem:not-found',
      title: 'Not Found',
      status: 404,
      code: 'not_found',
      correlation_id: 'corr_not_found',
    })} />);

    expect(screen.getByText('Resource unavailable')).toBeTruthy();
    expect(screen.queryByText('Not Found')).toBeNull();

    rerender(<StatusMessage state={new ApiProblem({
      type: 'urn:part-crm:problem:forbidden',
      title: 'Forbidden',
      status: 403,
      code: 'forbidden',
      correlation_id: 'corr_forbidden',
    })} />);

    expect(screen.getByText('Resource unavailable')).toBeTruthy();
    expect(screen.queryByText('Forbidden')).toBeNull();
  });
});
