// @vitest-environment jsdom
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { fakeTransport, frame, installDomStubs, mount } from './helpers/harness';

beforeEach(() => {
  installDomStubs();
  document.body.innerHTML = '';
});
afterEach(() => vi.unstubAllGlobals());

async function loaded() {
  const transport = fakeTransport();
  const mounted = mount(transport);
  transport.update('Table users { id int }', undefined, { saved: true });
  await frame();
  const internals = mounted.workbench as unknown as Record<string, () => void>;
  return {
    ...mounted,
    chrome: vi.spyOn(internals, 'renderChrome'),
    world: vi.spyOn(internals, 'renderWorld'),
    surface: mounted.root.querySelector('#surface') as HTMLElement,
    worldEl: mounted.root.querySelector('#world') as HTMLElement,
  };
}

describe('invalidação do render', () => {
  it('rolar o canvas só atualiza o transform (sem reconstruir cromo nem mundo)', async () => {
    const { surface, worldEl, chrome, world } = await loaded();
    const before = worldEl.style.transform;
    const firstCard = worldEl.querySelector('.card');
    expect(firstCard).not.toBeNull();

    surface.dispatchEvent(new WheelEvent('wheel', { deltaX: 40, deltaY: 25, cancelable: true }));
    surface.dispatchEvent(new WheelEvent('wheel', { deltaX: 10, deltaY: 5, cancelable: true }));
    await frame();

    expect(worldEl.style.transform).not.toBe(before);
    expect(chrome).not.toHaveBeenCalled();
    expect(world).not.toHaveBeenCalled();
    expect(worldEl.querySelector('.card')).toBe(firstCard);
  });

  it('zoom e edições continuam renderizando por completo', async () => {
    const { surface, chrome, world } = await loaded();
    surface.dispatchEvent(new WheelEvent('wheel', { deltaY: -40, ctrlKey: true, cancelable: true }));
    await frame();
    expect(chrome).toHaveBeenCalledTimes(1);
    expect(world).toHaveBeenCalledTimes(1);
  });

  it('um pedido completo no mesmo quadro vence o de câmera', async () => {
    const { surface, workbench, chrome } = await loaded();
    surface.dispatchEvent(new WheelEvent('wheel', { deltaY: 30, cancelable: true }));
    (workbench as unknown as { scheduleRender(): void }).scheduleRender();
    await frame();
    expect(chrome).toHaveBeenCalledTimes(1);
  });
});
