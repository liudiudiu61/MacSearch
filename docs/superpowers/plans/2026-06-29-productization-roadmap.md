# 麦搜产品化优化计划

> **给后续执行代理看的要求：** 执行本计划时，必须使用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans`，按任务逐项推进。步骤使用 `- [ ]` 复选框格式，便于跟踪状态。

**目标：** 把麦搜从“核心搜索能力可用”的本地搜索原型，推进到一个更美观、更流畅、更接近公众上架标准的 macOS 搜索应用。

**架构方向：** 保持现有 Tauri v2 + Vue 3 + Pinia + Rust command 边界。先优化搜索输入热路径，解决打字卡顿；再把 UI 重构成更像 macOS 专业工具的三栏结构；最后按 Everything / AnyTXT 的参考能力，补齐筛选、排序、快捷操作和索引管理。

**技术栈：** Tauri v2、Vue 3 `<script setup>`、Pinia、Tailwind、Rust command 层、配置 JSON、Vitest、Cargo tests。

---

## 当前状态

- [x] Phase 1：SQLite 数据模型、本地 FTS 基线、云端契约模型。
- [x] Phase 2：Rust 资源控制、文件扫描、FSEvents 边界、FastAPI NL2DSL/SSE 骨架。
- [x] Phase 3：Raycast 风格搜索壳、Tauri 文件名搜索、真实预览加载、Pinia 状态流。
- [x] Phase 4：资源约束和异常内容边界验证。
- [x] Phase 5：AnyTXT 风格内容索引 MVP、解析器边界、内容片段、待解析数量状态。
- [x] Phase 6：搜索输入流畅度与热路径稳定性。
- [x] Phase 7：面向公众使用的 macOS UI 信息架构。
- [ ] Phase 8：对齐 Everything / AnyTXT 的筛选、排序、动作和索引管理切片。

## 产品假设

1. 第一个公众可感知承诺仍然是“本地搜索即时响应”；内容搜索不能阻塞文字输入。
2. 当前输入卡顿很可能来自同步 `setQuery` 链路：每次输入都会启动 Tauri 搜索、把全局状态切到 `Building`、重新计算过滤结果、选中第一项，再触发预览内容加载。
3. 搜索体验应该先显示快速文件名结果，再异步追加或修正内容搜索结果。
4. 新增业务规则、筛选分类、解析器支持、路径排除、动作文案、状态码都必须保持配置驱动，不能硬编码。
5. UI 应该像原生 macOS 生产力工具，不做营销页，也不照搬 Windows 工具界面。

## 目标界面

```text
+--------------------------------------------------------------------------------+
| 工具栏：搜索框                              模式   筛选   索引状态             |
+---------------+--------------------------------------+-------------------------+
| 侧边栏        | 结果列表                             | 预览 / 检查器           |
| 全部          | 高密度虚拟列表                       | 内容命中高亮            |
| 文档          | 名称、路径、类型、修改时间、大小      | 元数据                  |
| PDF           | NAME / CONTENT 命中标识               | 快捷操作                |
| 代码          | 可排序字段或紧凑元信息                | 解析 / 索引状态         |
| 收藏目录      |                                      |                         |
+---------------+--------------------------------------+-------------------------+
| 状态栏：结果数量、搜索耗时、已索引文件数、待解析数量                           |
+--------------------------------------------------------------------------------+
```

## Phase 6：搜索输入流畅度

### Task 6.1：补充搜索交互性能测试

**文件：**
- 修改：`src/stores/searchStore.ts`
- 修改：`tests/localSearch.test.ts`
- 修改：`src/composables/useLocalSearch.ts`

- [x] **Step 1：编写“旧请求不能覆盖新结果”的失败测试**

增加一个测试，模拟两次快速输入，并断言只有最新一次搜索结果会进入可见结果。

```ts
it('keeps only the latest query results when searches resolve out of order', async () => {
  const first = deferred<SearchResult[]>();
  const second = deferred<SearchResult[]>();
  vi.spyOn(localSearch, 'searchFileNames')
    .mockReturnValueOnce(first.promise)
    .mockReturnValueOnce(second.promise);

  const store = useSearchStore();
  const firstSet = store.setQuery('name:first');
  const secondSet = store.setQuery('name:second');

  second.resolve([
    searchResultFixture({ id: 'second', name: 'second.md', path: '/work/second.md' })
  ]);
  first.resolve([
    searchResultFixture({ id: 'first', name: 'first.md', path: '/work/first.md' })
  ]);

  await Promise.all([firstSet, secondSet]);

  expect(store.results.map((item) => item.id)).toEqual(['second']);
});
```

- [x] **Step 2：运行聚焦测试，确认实现前失败**

运行：`npm test -- tests/localSearch.test.ts`

预期：FAIL。当前 store 在同一条输入链路里混入了预览、状态更新等副作用，也没有搜索耗时记录。

- [x] **Step 3：增加搜索耗时字段，不改变 command 契约**

在 `src/stores/searchStore.ts` 中增加状态字段：

```ts
lastSearchStartedAt: 0,
lastSearchElapsedMs: 0,
pendingPreviewPath: null as string | null
```

只为最新请求记录耗时：

```ts
const startedAt = performance.now();
...
if (requestId === this.searchRequestId) {
  this.lastSearchElapsedMs = Math.round(performance.now() - startedAt);
  this.isSearching = false;
}
```

- [x] **Step 4：把预览加载从文字输入链路中拆出去**

调整 `setQuery`：它只负责更新 `query`、发起搜索、写入结果、选中第一项，不再在输入路径上 `await loadSelectedPreview()`。

```ts
const first = this.visibleResults[0];
this.selectedId = first?.id ?? null;
if (first) {
  this.pendingPreviewPath = first.path;
  window.setTimeout(() => {
    if (this.pendingPreviewPath === first.path) {
      void this.loadSelectedPreview();
    }
  }, 80);
}
```

- [x] **Step 5：验证前端行为**

运行：`npm test -- tests/localSearch.test.ts`

预期：PASS，包括旧请求保护和现有 query mapping 测试。

### Task 6.2：给原生命令搜索加防抖，但不延迟文字显示

**文件：**
- 修改：`src/stores/searchStore.ts`
- 测试：`tests/localSearch.test.ts`

- [x] **Step 1：编写“输入先更新，搜索后触发”的失败测试**

```ts
it('updates query immediately before debounced runtime search resolves', async () => {
  vi.useFakeTimers();
  vi.spyOn(localSearch, 'searchFileNames').mockResolvedValue([]);

  const store = useSearchStore();
  const pending = store.setQuery('roadmap');

  expect(store.query).toBe('roadmap');
  expect(localSearch.searchFileNames).not.toHaveBeenCalled();

  await vi.advanceTimersByTimeAsync(80);
  await pending;

  expect(localSearch.searchFileNames).toHaveBeenCalledTimes(1);
});
```

- [x] **Step 2：实现短防抖，并保持配置驱动**

在 `config/search_syntax.json` 中增加前端输入行为配置，或在相邻配置文件中新增同类配置：

```json
{
  "searchDebounceMs": 80
}
```

store 读取这个配置后再调用 `searchFileNames`。等待过程必须能被 `searchRequestId` 取消。

- [x] **Step 3：验证**

运行：`npm test -- tests/localSearch.test.ts`

预期：PASS。文字输入立即更新；原生命令搜索在防抖窗口后启动。

### Task 6.3：拆分文件名结果与内容结果加载

**文件：**
- 修改：`src/composables/useLocalSearch.ts`
- 修改：`src/stores/searchStore.ts`
- 修改：`src-tauri/src/commands.rs`
- 修改：`src-tauri/src/search.rs`
- 测试：`tests/localSearch.test.ts`
- 测试：`src-tauri/src/commands.rs`

- [x] **Step 1：给请求契约增加搜索模式字段**

扩展请求类型：

```ts
export type SearchMode = 'filename' | 'content' | 'mixed';
```

Rust 请求结构也镜像这个字符串字段。模式必须来自 command request，不能在 UI 里写死推断。

- [x] **Step 2：实现文件名优先**

当模式为 `mixed` 时，先用 `filename` 调用 runtime command，立即更新可见结果；再用 `content` 调用并按 path 合并内容命中。

- [x] **Step 3：验证**

运行：

```bash
npm test -- tests/localSearch.test.ts
cargo test
npm run build
```

预期：文件名结果能先于内容结果渲染；过期的内容结果不能覆盖更新查询。

## Phase 7：公众可用的 macOS UI

### Task 7.1：引入三栏信息架构

**文件：**
- 修改：`src/components/SearchShell.vue`
- 修改：`src/styles.css`
- 修改：`src/types/search.ts`
- 验证：`npm run build`

- [x] **Step 1：先保护现有搜索行为**

改 UI 前先运行：

```bash
npm test
npm run build
```

预期：PASS。如果当前有失败，先修复既有失败，再改布局。

- [x] **Step 2：把单个居中面板改成应用窗口布局**

使用：
- 左侧栏承载搜索范围和文件类型；
- 中间区域承载虚拟化结果列表；
- 右侧区域承载预览和检查器；
- 底部状态栏承载结果数与搜索耗时。

不要增加营销文案、巨大卡片或装饰性渐变。

- [x] **Step 3：控件保持原生、紧凑、高密度**

模式切换用 segmented control，动作按钮用紧凑图标/文字按钮，虚拟列表保持稳定行高。

- [x] **Step 4：验证视觉构建**

运行：`npm run build`

预期：PASS，没有 TypeScript 模板错误。

### Task 7.2：增加用于排序和快速扫读的结果元数据

**文件：**
- 修改：`src/types/search.ts`
- 修改：`src/composables/useLocalSearch.ts`
- 修改：`src/components/SearchShell.vue`
- 修改：`src-tauri/src/commands.rs`
- 测试：`tests/localSearch.test.ts`
- 测试：`src-tauri/src/commands.rs`

- [x] **Step 1：扩展结果契约**

增加 `sizeBytes`、`modifiedAtUnix` 和 `kind` 字段。`kind` 必须从扩展名分组配置推导，不能写死。

- [x] **Step 2：在不增加行高的前提下渲染元数据**

结果行展示名称、路径、命中类型、扩展名/类型、修改时间、大小，布局要适合快速扫读。

- [x] **Step 3：验证**

运行：

```bash
cargo test
npm test
npm run build
```

预期：Rust command 测试和 TS 映射测试都能拿到元数据。

### Task 7.3：完善空状态、加载状态、错误状态和权限状态

**文件：**
- 修改：`src/components/SearchShell.vue`
- 修改：`src/stores/searchStore.ts`
- 修改：`src/composables/useIndexerStatus.ts`
- 测试：`tests/localSearch.test.ts`

- [x] **Step 1：用配置定义状态文案**

创建或扩展一个配置文件管理 UI 状态文案。长期存在的状态文案不要硬编码在 Vue 组件里。

- [x] **Step 2：增加紧凑内联状态**

覆盖：
- 尚未建立索引；
- 正在索引；
- 因资源策略挂起；
- 存在解析失败；
- 无搜索结果；
- runtime command 不可用。

- [x] **Step 3：验证**

运行：`npm test && npm run build`

预期：所有状态都通过配置映射，并且渲染时不造成布局跳动。

## Phase 8：Everything / AnyTXT 功能对齐切片

### Task 8.1：增加搜索模式与文件类型筛选

**文件：**
- 修改：`config/search_syntax.json`
- 修改：`src/composables/useQueryParser.ts`
- 修改：`src/composables/useSearchFilter.ts`
- 修改：`src/components/SearchShell.vue`
- 测试：`tests/localSearch.test.ts`

- [x] **Step 1：增加配置驱动的文件类型分组**

示例配置形态：

```json
{
  "fileTypeGroups": {
    "documents": [".md", ".txt", ".docx", ".pdf"],
    "code": [".ts", ".tsx", ".rs", ".py", ".json"],
    "tables": [".csv", ".xlsx"]
  }
}
```

- [x] **Step 2：增加 UI 筛选**

侧边栏筛选更新 filter state，不直接改写用户输入的原始 query。

- [x] **Step 3：验证**

运行：`npm test && npm run build`

预期：文件类型归组来自配置，筛选条件能和输入查询共同生效。

### Task 8.2：增加排序与键盘优先动作

**文件：**
- 修改：`src/stores/searchStore.ts`
- 修改：`src/components/SearchShell.vue`
- 测试：`tests/localSearch.test.ts`

- [x] **Step 1：增加排序状态**

支持按相关性、名称、修改时间、大小、路径排序。搜索结果默认按相关性排序。

- [x] **Step 2：补齐预期动作**

支持：
- Enter：打开；
- Cmd+Enter：在 Finder 中显示；
- Cmd+C：复制路径；
- 右键菜单：打开、显示、复制路径。

当前键盘动作已经存在，实现时必须保留，同时增加可见的紧凑动作入口。

- [x] **Step 3：验证**

运行：`npm test && npm run build`

预期：排序结果稳定，现有键盘快捷键继续工作。

### Task 8.3：增加索引设置入口

**文件：**
- 修改：`config/indexer_policy.json`
- 修改：`src-tauri/src/commands.rs`
- 修改：`src-tauri/src/indexer.rs`
- 新建：`src/components/IndexSettingsPanel.vue`
- 测试：`cargo test`
- 验证：`npm run build`

- [x] **Step 1：通过 command 读取当前索引策略**

增加一个 command，返回已配置的扫描根目录、监听根目录、排除路径、最大解析大小、支持的文本扩展名和内容索引设置。

- [x] **Step 2：先做只读设置面板**

第一版设置面板只读，让用户先理解当前索引范围。写入配置需要更强校验，所以不要在这个切片里急着做。

- [x] **Step 3：验证**

运行：

```bash
cargo test
npm run build
```

预期：设置面板能反映配置内容，不出现硬编码路径规则。

## 上架前验证清单

- [ ] `cargo test`
- [ ] `cargo fmt --check`
- [ ] `cargo build`
- [ ] `npm test`
- [ ] `npm run build`
- [ ] 手动输入检查：长时间连续输入不会明显卡住字符。
- [ ] 手动搜索检查：文件名结果先于内容预览工作出现。
- [ ] 手动 UI 检查：桌面和窄窗口布局都没有文字重叠。
- [ ] 手动动作检查：打开、在 Finder 中显示、复制路径均可用。
- [ ] 隐私检查：索引内容不离开本机。

## 推荐执行顺序

1. Task 6.1：度量并稳定 query 热路径。
2. Task 6.2：给原生搜索加防抖，同时保持输入立即显示。
3. Task 6.3：拆分文件名优先和内容后续加载。
4. Task 7.1：把搜索壳重构成公众可用的三栏应用布局。
5. Task 7.2：增加高密度专业结果列表需要的元数据。
6. Task 8.1：增加模式与类型筛选。
7. Task 8.2：增加排序和可见动作。
8. Task 7.3 与 Task 8.3：打磨状态和设置入口。

## 产品风险

1. 公众用户可以晚一点接受高级语法缺失，但很难接受输入框卡顿。
2. 内容解析必须保持异步且有明确队列状态；如果每次按键都阻塞预览或内容提取，应用会一直显得沉重。
3. PDF / Office / OCR 很有诱惑力，但应该等 UI 和热路径稳定后，再沿现有 parser boundary 扩展。
4. 写入 `indexer_policy.json` 的设置功能需要更强校验，所以第一版设置切片只做只读展示。
