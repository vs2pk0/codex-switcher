import test from 'node:test';
import assert from 'node:assert/strict';
import { orderMigrationAccounts } from '../src/opencodex/accounts.ts';

test('OpenCodex follows home order without mutating scan results', () => {
  const accounts = ['a', 'b', 'c', 'new1', 'new2'].map(sourceId => ({ sourceId }));
  assert.deepEqual(orderMigrationAccounts(accounts, ['c', 'a', 'b']).map(a => a.sourceId), ['c', 'a', 'b', 'new1', 'new2']);
  assert.deepEqual(accounts.map(a => a.sourceId), ['a', 'b', 'c', 'new1', 'new2']);
  assert.deepEqual(orderMigrationAccounts(accounts, []).map(a => a.sourceId), accounts.map(a => a.sourceId));
  assert.deepEqual(orderMigrationAccounts(accounts.filter(a => a.sourceId !== 'a'), ['b', 'a', 'c']).map(a => a.sourceId), ['b', 'c', 'new1', 'new2']);
});
