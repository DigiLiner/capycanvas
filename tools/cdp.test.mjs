import test from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {launchChrome, prepareWorkers} from './cdp.mjs';

test('worker observers precede module execution and close rejects event waiters', async t => {
  const server = createServer((request, response) => {
    response.setHeader('Content-Type', request.url === '/worker.js' ? 'text/javascript' : 'text/html');
    response.end(request.url === '/worker.js'
      ? "globalThis.__observedAtStart=[...(globalThis.__observers??[])];globalThis.__workerStarted(JSON.stringify(__observedAtStart));"
      : '<!doctype html><title>Worker preparation</title>');
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  let cdp, first, second;
  t.after(async () => {
    try {
      await Promise.allSettled([first?.close(), second?.close()]);
      if (cdp) await cdp.close();
    } finally {
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    }
  });
  cdp = await launchChrome(['--headless=new'], {timeout: 180000});
  first = prepareWorkers(cdp, async session => {
    await cdp.call('Runtime.addBinding', {name: '__workerStarted'}, session);
    const value = await cdp.call('Runtime.evaluate', {
      expression: "(globalThis.__observers??=[]).push('first')",
    }, session);
    assert.equal(value.exceptionDetails, undefined);
  });
  second = prepareWorkers(cdp, async session => {
    const value = await cdp.call('Runtime.evaluate', {
      expression: "(globalThis.__observers??=[]).push('second')",
    }, session);
    assert.equal(value.exceptionDetails, undefined);
  });
  await cdp.attachPage();
  await cdp.call('Page.enable');
  const loaded = cdp.once('Page.loadEventFired', 180000);
  const url = `http://127.0.0.1:${server.address().port}`;
  await cdp.call('Page.navigate', {url});
  await loaded;
  await Promise.all([first.start(), second.start()]);
  const started = cdp.once('Runtime.bindingCalled', 180000);
  await cdp.call('Runtime.evaluate', {
    expression: `globalThis.worker=new Worker(${JSON.stringify(url + '/worker.js')},{type:'module'});true`,
  });
  const observed = await started;
  assert.equal(observed.name, '__workerStarted');
  assert.deepEqual(JSON.parse(observed.payload).sort(), ['first', 'second']);
  await Promise.all([first.check(), second.check()]);
  await first.close();first = undefined;
  await second.close();second = undefined;
  const waiter = cdp.once('CapyNeverEmittedEvent', 180000);
  const rejected = assert.rejects(waiter, /Chrome exited|CDP closed/);
  await cdp.close();cdp = undefined;
  await rejected;
});
