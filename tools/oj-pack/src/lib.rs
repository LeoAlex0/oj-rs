use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cargo_metadata::MetadataCommand;
use proc_macro2::{Delimiter, Ident, TokenStream, TokenTree};
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, File, Item, ItemMacro, ItemUse, Token, Type, UseTree};

type BoxError = Box<dyn std::error::Error>;
type ItemKey = usize;

#[derive(Debug, Clone)]
pub struct PackOptions {
    pub check: bool,
    pub minify: bool,
    pub max_bytes: Option<usize>,
    pub warn_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub name: String,
    pub crate_name: String,
    pub edition: String,
    pub lib_path: Option<PathBuf>,
    bins: BTreeMap<String, PathBuf>,
}

impl PackageInfo {
    pub fn load(project_root: &Path) -> Result<Self, BoxError> {
        let metadata = MetadataCommand::new()
            .current_dir(project_root)
            .no_deps()
            .exec()?;
        let package = metadata
            .root_package()
            .or_else(|| metadata.packages.first())
            .ok_or("cargo metadata returned no packages")?;

        let mut bins = BTreeMap::new();
        let mut lib_path = None;
        let mut crate_name = package.name.replace('-', "_");
        let mut edition = package.edition.to_string();

        for target in &package.targets {
            if target.is_lib() {
                lib_path = Some(target.src_path.clone().into_std_path_buf());
                crate_name = target.name.replace('-', "_");
                edition = target.edition.to_string();
            } else if target.is_bin() {
                bins.insert(
                    target.name.to_string(),
                    target.src_path.clone().into_std_path_buf(),
                );
            }
        }

        Ok(Self {
            name: package.name.to_string(),
            crate_name,
            edition,
            lib_path,
            bins,
        })
    }

    pub fn bin_names(&self) -> impl Iterator<Item = &str> {
        self.bins.keys().map(String::as_str)
    }
}

pub fn pack_project(
    project_root: &Path,
    bin_name: &str,
    options: PackOptions,
) -> Result<String, BoxError> {
    let package = PackageInfo::load(project_root)?;
    let bin_path = package
        .bins
        .get(bin_name)
        .ok_or_else(|| format!("unknown binary `{bin_name}`"))?;

    let mut bin_file = parse_file(bin_path)?;
    clean_file(&mut bin_file);

    let roots = collect_solution_roots(&bin_file, &package.crate_name);
    let include_library = !roots.is_empty();
    RewriteCrateName::new(&package.crate_name).visit_file_mut(&mut bin_file);
    bin_file
        .items
        .retain(|item| !matches!(item, Item::ExternCrate(ec) if ec.ident == package.crate_name));

    let mut output_items = Vec::new();
    if include_library {
        let lib_path = package
            .lib_path
            .as_ref()
            .ok_or("binary references the library crate, but no lib target exists")?;
        let library = Library::load(lib_path)?;
        let kept = library.eliminate_dead_items(&roots);
        output_items.extend(library.render(&kept)?);
    }
    output_items.extend(bin_file.items);

    let tokens = quote! { #(#output_items)* };
    let output = if options.minify {
        let mut s = minify(tokens);
        s.push('\n');
        s
    } else {
        prettyplease::unparse(&syn::parse2::<File>(tokens)?)
    };

    if options.check {
        check_rustc(project_root, &package.edition, &output)?;
    }

    if let Some(max) = options.max_bytes {
        if output.len() > max {
            return Err(format!(
                "packed output is {} bytes, exceeding --max-bytes {max}",
                output.len()
            )
            .into());
        }
    } else if output.len() > options.warn_bytes {
        eprintln!(
            "oj-pack: warning: packed output is {} bytes, exceeding {} bytes",
            output.len(),
            options.warn_bytes
        );
    }

    Ok(output)
}

fn parse_file(path: &Path) -> Result<File, BoxError> {
    let source = fs::read_to_string(path)?;
    syn::parse_file(&source)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()).into())
}

fn clean_file(file: &mut File) {
    file.attrs.retain(keep_attr);
    file.items = std::mem::take(&mut file.items)
        .into_iter()
        .filter_map(clean_item)
        .collect();
}

fn clean_items(items: Vec<Item>) -> Vec<Item> {
    items.into_iter().filter_map(clean_item).collect()
}

fn clean_item(mut item: Item) -> Option<Item> {
    if has_cfg_test(attrs_of(&item)) {
        return None;
    }
    attrs_of_mut(&mut item).retain(keep_attr);
    match &mut item {
        Item::Mod(module) => {
            if let Some((_, items)) = &mut module.content {
                *items = clean_items(std::mem::take(items));
            }
        }
        Item::Trait(item) => {
            for inner in &mut item.items {
                attrs_of_trait_item_mut(inner).retain(keep_attr);
            }
        }
        Item::Impl(item) => {
            for inner in &mut item.items {
                attrs_of_impl_item_mut(inner).retain(keep_attr);
            }
        }
        _ => {}
    }
    Some(item)
}

fn keep_attr(attr: &Attribute) -> bool {
    !attr.path().is_ident("doc")
}

fn has_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg") && attr.meta.to_token_stream().to_string().contains("test")
    })
}

fn attrs_of(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        Item::Verbatim(_) => &[],
        _ => &[],
    }
}

fn attrs_of_mut(item: &mut Item) -> &mut Vec<Attribute> {
    match item {
        Item::Const(item) => &mut item.attrs,
        Item::Enum(item) => &mut item.attrs,
        Item::ExternCrate(item) => &mut item.attrs,
        Item::Fn(item) => &mut item.attrs,
        Item::ForeignMod(item) => &mut item.attrs,
        Item::Impl(item) => &mut item.attrs,
        Item::Macro(item) => &mut item.attrs,
        Item::Mod(item) => &mut item.attrs,
        Item::Static(item) => &mut item.attrs,
        Item::Struct(item) => &mut item.attrs,
        Item::Trait(item) => &mut item.attrs,
        Item::TraitAlias(item) => &mut item.attrs,
        Item::Type(item) => &mut item.attrs,
        Item::Union(item) => &mut item.attrs,
        Item::Use(item) => &mut item.attrs,
        Item::Verbatim(_) => panic!("verbatim items have no attributes"),
        _ => panic!("unsupported item kind"),
    }
}

fn attrs_of_trait_item_mut(item: &mut syn::TraitItem) -> &mut Vec<Attribute> {
    match item {
        syn::TraitItem::Const(item) => &mut item.attrs,
        syn::TraitItem::Fn(item) => &mut item.attrs,
        syn::TraitItem::Type(item) => &mut item.attrs,
        syn::TraitItem::Macro(item) => &mut item.attrs,
        syn::TraitItem::Verbatim(_) => panic!("verbatim trait items have no attributes"),
        _ => panic!("unsupported trait item kind"),
    }
}

fn attrs_of_impl_item_mut(item: &mut syn::ImplItem) -> &mut Vec<Attribute> {
    match item {
        syn::ImplItem::Const(item) => &mut item.attrs,
        syn::ImplItem::Fn(item) => &mut item.attrs,
        syn::ImplItem::Type(item) => &mut item.attrs,
        syn::ImplItem::Macro(item) => &mut item.attrs,
        syn::ImplItem::Verbatim(_) => panic!("verbatim impl items have no attributes"),
        _ => panic!("unsupported impl item kind"),
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct RootRef {
    module: Vec<String>,
    name: String,
    glob: bool,
    used_names: BTreeSet<String>,
}

fn collect_solution_roots(file: &File, crate_name: &str) -> Vec<RootRef> {
    let idents = collect_idents(file.to_token_stream());
    let mut roots = Vec::new();

    for item in &file.items {
        if let Item::Use(item) = item {
            collect_use_roots(&item.tree, crate_name, Vec::new(), &idents, &mut roots);
        } else {
            collect_path_roots(item.to_token_stream(), crate_name, &mut roots);
        }
    }

    roots.sort();
    roots.dedup();
    roots
}

fn collect_use_roots(
    tree: &UseTree,
    crate_name: &str,
    mut prefix: Vec<String>,
    idents: &HashSet<String>,
    roots: &mut Vec<RootRef>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_use_roots(&path.tree, crate_name, prefix, idents, roots);
        }
        UseTree::Name(name) => {
            prefix.push(name.ident.to_string());
            push_use_root(prefix, crate_name, false, idents, roots);
        }
        UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            push_use_root(prefix, crate_name, false, idents, roots);
        }
        UseTree::Glob(_) => push_use_root(prefix, crate_name, true, idents, roots),
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_roots(item, crate_name, prefix.clone(), idents, roots);
            }
        }
    }
}

fn push_use_root(
    mut path: Vec<String>,
    crate_name: &str,
    glob: bool,
    idents: &HashSet<String>,
    roots: &mut Vec<RootRef>,
) {
    if path.first().is_none_or(|first| first != crate_name) {
        return;
    }
    path.remove(0);
    if glob {
        roots.push(RootRef {
            module: path,
            name: String::new(),
            glob: true,
            used_names: idents.iter().cloned().collect(),
        });
    } else if let Some(name) = path.pop() {
        if idents.contains(&name) {
            roots.push(RootRef {
                module: path,
                name,
                glob: false,
                used_names: BTreeSet::new(),
            });
        }
    }
}

fn collect_path_roots(tokens: TokenStream, crate_name: &str, roots: &mut Vec<RootRef>) {
    let flat = flatten_tokens(tokens);
    for window_start in 0..flat.len() {
        if flat[window_start] != crate_name {
            continue;
        }
        let mut parts = Vec::new();
        let mut i = window_start + 1;
        while i + 1 < flat.len() && flat[i] == "::" {
            if is_ident_like(&flat[i + 1]) {
                parts.push(flat[i + 1].clone());
                i += 2;
            } else {
                break;
            }
        }
        if let Some(name) = parts.pop() {
            roots.push(RootRef {
                module: parts,
                name,
                glob: false,
                used_names: BTreeSet::new(),
            });
        }
    }
}

struct Library {
    root: Module,
    records: Vec<Record>,
    name_index: HashMap<String, Vec<ItemKey>>,
    modules: HashMap<Vec<String>, ModuleIndex>,
}

type ModuleIndex = Vec<usize>;

struct Module {
    name: Option<String>,
    path: Vec<String>,
    items: Vec<ModuleItem>,
}

enum ModuleItem {
    Use(ItemUse),
    Item(ItemKey),
    Child(Module),
}

#[derive(Clone)]
struct Record {
    item: Item,
    names: BTreeSet<String>,
    refs: BTreeSet<String>,
    local_refs: BTreeSet<String>,
    impl_self_locals: BTreeSet<String>,
    impl_trait_name: Option<String>,
    kind: RecordKind,
    is_pub: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RecordKind {
    Named,
    Trait,
    Impl,
    MacroDef,
    MacroUse,
}

fn self_like_macro_dependency(kind: RecordKind) -> bool {
    matches!(kind, RecordKind::Named | RecordKind::MacroDef)
}

impl Library {
    fn load(lib_path: &Path) -> Result<Self, BoxError> {
        let mut records = Vec::new();
        let root = load_module(None, Vec::new(), lib_path, &mut records)?;
        let mut name_index: HashMap<String, Vec<ItemKey>> = HashMap::new();
        for (key, record) in records.iter().enumerate() {
            for name in &record.names {
                name_index.entry(name.clone()).or_default().push(key);
            }
        }

        let macro_def_refs: HashMap<String, BTreeSet<String>> = records
            .iter()
            .filter(|record| record.kind == RecordKind::MacroDef)
            .flat_map(|record| {
                let refs = record
                    .refs
                    .iter()
                    .filter(|name| {
                        name_index.get(*name).is_some_and(|keys| {
                            keys.iter()
                                .any(|key| self_like_macro_dependency(records[*key].kind))
                        })
                    })
                    .cloned()
                    .collect::<BTreeSet<_>>();
                record
                    .names
                    .iter()
                    .map(|name| (name.clone(), refs.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();

        for record in &mut records {
            if record.kind == RecordKind::MacroUse {
                if let Some(name) = macro_call_name(&record.item) {
                    if let Some(refs) = macro_def_refs.get(&name) {
                        record.refs.extend(refs.iter().cloned());
                    }
                }
            }
            record.local_refs = record
                .refs
                .iter()
                .filter(|name| name_index.contains_key(*name))
                .cloned()
                .collect();
            if matches!(record.kind, RecordKind::Impl) {
                record
                    .impl_self_locals
                    .retain(|name| name_index.contains_key(name));
                if record
                    .impl_trait_name
                    .as_ref()
                    .is_some_and(|name| !name_index.contains_key(name))
                {
                    record.impl_trait_name = None;
                }
            }
        }

        let mut library = Self {
            root,
            records,
            name_index,
            modules: HashMap::new(),
        };
        library.rebuild_module_index();
        Ok(library)
    }

    fn rebuild_module_index(&mut self) {
        self.modules.clear();
        index_modules(&self.root, Vec::new(), &mut self.modules);
    }

    fn eliminate_dead_items(&self, roots: &[RootRef]) -> BTreeSet<ItemKey> {
        let mut kept = BTreeSet::new();
        let mut work = VecDeque::new();

        for root in roots {
            if root.glob {
                for key in self.exported_for_glob(&root.module, &root.used_names) {
                    enqueue(key, &mut kept, &mut work);
                }
            } else {
                for key in self.exported_matching(&root.module, Some(&root.name)) {
                    enqueue(key, &mut kept, &mut work);
                }
            }
        }

        loop {
            while let Some(key) = work.pop_front() {
                for dep in self.dependencies_of(key) {
                    enqueue(dep, &mut kept, &mut work);
                }
            }

            let kept_names = self.kept_names(&kept);
            let mut changed = false;
            for (key, record) in self.records.iter().enumerate() {
                if kept.contains(&key) {
                    continue;
                }
                if self.should_keep_by_reachability(record, &kept_names) {
                    enqueue(key, &mut kept, &mut work);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        kept
    }

    fn dependencies_of(&self, key: ItemKey) -> Vec<ItemKey> {
        self.records[key]
            .local_refs
            .iter()
            .filter_map(|name| self.name_index.get(name))
            .flat_map(|keys| keys.iter().copied())
            .collect()
    }

    fn kept_names(&self, kept: &BTreeSet<ItemKey>) -> BTreeSet<String> {
        kept.iter()
            .flat_map(|key| self.records[*key].names.iter().cloned())
            .collect()
    }

    fn should_keep_by_reachability(&self, record: &Record, kept_names: &BTreeSet<String>) -> bool {
        match record.kind {
            RecordKind::Impl => {
                let self_reachable = record.impl_self_locals.is_empty()
                    || record
                        .impl_self_locals
                        .iter()
                        .any(|name| kept_names.contains(name));
                match &record.impl_trait_name {
                    Some(trait_name) => self_reachable && kept_names.contains(trait_name),
                    None => self_reachable,
                }
            }
            RecordKind::MacroUse => record.local_refs.iter().any(|name| {
                kept_names.contains(name) && macro_call_name(&record.item).as_ref() != Some(name)
            }),
            _ => false,
        }
    }

    fn exported_matching(&self, module: &[String], name: Option<&str>) -> Vec<ItemKey> {
        let Some(index) = self.modules.get(module) else {
            return Vec::new();
        };
        let Some(module) = self.module_by_index(index) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        self.collect_exports(module, name, &mut out);
        out.sort_unstable();
        out.dedup();
        out
    }

    fn exported_for_glob(&self, module: &[String], used_names: &BTreeSet<String>) -> Vec<ItemKey> {
        let Some(index) = self.modules.get(module) else {
            return Vec::new();
        };
        let Some(module) = self.module_by_index(index) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        self.collect_glob_exports(module, used_names, &mut out);
        out.sort_unstable();
        out.dedup();
        out
    }

    fn collect_exports(&self, module: &Module, name: Option<&str>, out: &mut Vec<ItemKey>) {
        for item in &module.items {
            match item {
                ModuleItem::Item(key) => {
                    let record = &self.records[*key];
                    if record.is_pub && name.is_none_or(|name| record.names.contains(name)) {
                        out.push(*key);
                    }
                }
                ModuleItem::Use(item) if is_pub_use(item) => {
                    collect_reexport_roots(&item.tree, &module.path, name, self, out);
                }
                ModuleItem::Child(child)
                    if name.is_some_and(|name| child.name.as_deref() == Some(name)) =>
                {
                    self.collect_exports(child, None, out);
                }
                _ => {}
            }
        }
    }

    fn collect_glob_exports(
        &self,
        module: &Module,
        used_names: &BTreeSet<String>,
        out: &mut Vec<ItemKey>,
    ) {
        for item in &module.items {
            match item {
                ModuleItem::Item(key) => {
                    let record = &self.records[*key];
                    let name_used = record.names.iter().any(|name| used_names.contains(name));
                    if record.is_pub && (name_used || record.kind == RecordKind::Trait) {
                        out.push(*key);
                    }
                }
                ModuleItem::Use(item) if is_pub_use(item) => {
                    collect_reexport_glob_roots(&item.tree, &module.path, used_names, self, out);
                }
                ModuleItem::Child(child)
                    if child
                        .name
                        .as_ref()
                        .is_some_and(|name| used_names.contains(name)) =>
                {
                    self.collect_glob_exports(child, used_names, out);
                }
                _ => {}
            }
        }
    }

    fn module_by_index(&self, index: &[usize]) -> Option<&Module> {
        let mut module = &self.root;
        for pos in index {
            match module.items.get(*pos)? {
                ModuleItem::Child(child) => module = child,
                _ => return None,
            }
        }
        Some(module)
    }

    fn render(&self, kept: &BTreeSet<ItemKey>) -> Result<Vec<Item>, BoxError> {
        let kept_names = self.kept_names(kept);
        self.render_module_items(&self.root, kept, &kept_names, true)
    }

    fn render_module_items(
        &self,
        module: &Module,
        kept: &BTreeSet<ItemKey>,
        kept_names: &BTreeSet<String>,
        is_root: bool,
    ) -> Result<Vec<Item>, BoxError> {
        let mut out = Vec::new();
        let module_has_code = self.module_contains_output(module, kept, kept_names);
        for item in &module.items {
            match item {
                ModuleItem::Use(item) if module_has_code && is_pub_use(item) => {
                    if let Some(item) = self.prune_reexport_use(item, &module.path, kept) {
                        out.push(Item::Use(item));
                    }
                }
                ModuleItem::Use(item) if module_has_code => {
                    if let Some(item) = self.prune_use(item, &module.path, kept) {
                        out.push(Item::Use(item));
                    }
                }
                ModuleItem::Use(_) => {}
                ModuleItem::Item(key) if kept.contains(key) => {
                    out.push(self.records[*key].item.clone());
                }
                ModuleItem::Item(_) => {}
                ModuleItem::Child(child)
                    if self.module_contains_output(child, kept, kept_names) =>
                {
                    let child_items = self.render_module_items(child, kept, kept_names, false)?;
                    let ident = Ident::new(
                        child.name.as_deref().unwrap_or("root"),
                        proc_macro2::Span::call_site(),
                    );
                    let module_item: Item = syn::parse_quote! {
                        pub mod #ident {
                            #(#child_items)*
                        }
                    };
                    out.push(module_item);
                }
                ModuleItem::Child(_) => {}
            }
        }
        if !is_root && out.is_empty() {
            return Ok(Vec::new());
        }
        Ok(out)
    }

    fn prune_reexport_use(
        &self,
        item: &ItemUse,
        module_path: &[String],
        kept: &BTreeSet<ItemKey>,
    ) -> Option<ItemUse> {
        let mut item = item.clone();
        item.tree = self.prune_reexport_tree(&item.tree, module_path, kept)?;
        Some(item)
    }

    fn prune_use(
        &self,
        item: &ItemUse,
        module_path: &[String],
        kept: &BTreeSet<ItemKey>,
    ) -> Option<ItemUse> {
        let mut item = item.clone();
        item.tree = self.prune_use_tree(&item.tree, module_path, kept)?;
        Some(item)
    }

    fn prune_use_tree(
        &self,
        tree: &UseTree,
        module_path: &[String],
        kept: &BTreeSet<ItemKey>,
    ) -> Option<UseTree> {
        match tree {
            UseTree::Path(path) => {
                let child_path = resolve_use_path_segment(module_path, &path.ident);
                let mut path = path.clone();
                path.tree = Box::new(self.prune_use_tree(&path.tree, &child_path, kept)?);
                Some(UseTree::Path(path))
            }
            UseTree::Name(name) => {
                let exported = self.exported_matching(module_path, Some(&name.ident.to_string()));
                (exported.is_empty() || exported.into_iter().any(|key| kept.contains(&key)))
                    .then(|| tree.clone())
            }
            UseTree::Rename(rename) => {
                let exported = self.exported_matching(module_path, Some(&rename.ident.to_string()));
                (exported.is_empty() || exported.into_iter().any(|key| kept.contains(&key)))
                    .then(|| tree.clone())
            }
            UseTree::Glob(_) => {
                let exported = self.exported_matching(module_path, None);
                (exported.is_empty() || exported.into_iter().any(|key| kept.contains(&key)))
                    .then(|| tree.clone())
            }
            UseTree::Group(group) => {
                let mut items = Punctuated::<UseTree, Token![,]>::new();
                for item in &group.items {
                    if let Some(item) = self.prune_use_tree(item, module_path, kept) {
                        items.push(item);
                    }
                }
                if items.is_empty() {
                    None
                } else {
                    let mut group = group.clone();
                    group.items = items;
                    Some(UseTree::Group(group))
                }
            }
        }
    }

    fn prune_reexport_tree(
        &self,
        tree: &UseTree,
        module_path: &[String],
        kept: &BTreeSet<ItemKey>,
    ) -> Option<UseTree> {
        match tree {
            UseTree::Path(path) => {
                let child_path = resolve_use_path_segment(module_path, &path.ident);
                let mut path = path.clone();
                path.tree = Box::new(self.prune_reexport_tree(&path.tree, &child_path, kept)?);
                Some(UseTree::Path(path))
            }
            UseTree::Name(name) => self
                .exported_matching(module_path, Some(&name.ident.to_string()))
                .into_iter()
                .any(|key| kept.contains(&key))
                .then(|| tree.clone()),
            UseTree::Rename(rename) => self
                .exported_matching(module_path, Some(&rename.ident.to_string()))
                .into_iter()
                .any(|key| kept.contains(&key))
                .then(|| tree.clone()),
            UseTree::Glob(_) => self
                .exported_matching(module_path, None)
                .into_iter()
                .any(|key| kept.contains(&key))
                .then(|| tree.clone()),
            UseTree::Group(group) => {
                let mut items = Punctuated::<UseTree, Token![,]>::new();
                for item in &group.items {
                    if let Some(item) = self.prune_reexport_tree(item, module_path, kept) {
                        items.push(item);
                    }
                }
                if items.is_empty() {
                    None
                } else {
                    let mut group = group.clone();
                    group.items = items;
                    Some(UseTree::Group(group))
                }
            }
        }
    }

    fn module_contains_output(
        &self,
        module: &Module,
        kept: &BTreeSet<ItemKey>,
        kept_names: &BTreeSet<String>,
    ) -> bool {
        module.items.iter().any(|item| match item {
            ModuleItem::Item(key) => kept.contains(key),
            ModuleItem::Child(child) => self.module_contains_output(child, kept, kept_names),
            ModuleItem::Use(item) if is_pub_use(item) => {
                self.reexport_reaches_kept(item, &module.path, kept, kept_names)
            }
            ModuleItem::Use(_) => false,
        })
    }

    fn reexport_reaches_kept(
        &self,
        item: &ItemUse,
        module_path: &[String],
        kept: &BTreeSet<ItemKey>,
        kept_names: &BTreeSet<String>,
    ) -> bool {
        let mut exported = Vec::new();
        collect_reexport_glob_roots(&item.tree, module_path, kept_names, self, &mut exported);
        exported.into_iter().any(|key| kept.contains(&key))
    }
}

fn index_modules(
    module: &Module,
    index: Vec<usize>,
    modules: &mut HashMap<Vec<String>, ModuleIndex>,
) {
    modules.insert(module.path.clone(), index.clone());
    for (pos, item) in module.items.iter().enumerate() {
        if let ModuleItem::Child(child) = item {
            let mut child_index = index.clone();
            child_index.push(pos);
            index_modules(child, child_index, modules);
        }
    }
}

fn enqueue(key: ItemKey, kept: &mut BTreeSet<ItemKey>, work: &mut VecDeque<ItemKey>) {
    if kept.insert(key) {
        work.push_back(key);
    }
}

fn load_module(
    name: Option<String>,
    path: Vec<String>,
    file_path: &Path,
    records: &mut Vec<Record>,
) -> Result<Module, BoxError> {
    let mut file = parse_file(file_path)?;
    clean_file(&mut file);
    let base_dir = module_base_dir(file_path, path.is_empty());
    let mut items = Vec::new();

    for item in file.items {
        match item {
            Item::Mod(module) if module.semi.is_some() => {
                let child_name = module.ident.to_string();
                let mut child_path = path.clone();
                child_path.push(child_name.clone());
                let child_file = find_module_file(&base_dir, &child_name)?;
                items.push(ModuleItem::Child(load_module(
                    Some(child_name),
                    child_path,
                    &child_file,
                    records,
                )?));
            }
            Item::Mod(mut module) => {
                let child_name = module.ident.to_string();
                let mut child_path = path.clone();
                child_path.push(child_name.clone());
                let child_items = module
                    .content
                    .take()
                    .map(|(_, items)| items)
                    .unwrap_or_default();
                let child = load_inline_module(Some(child_name), child_path, child_items, records)?;
                items.push(ModuleItem::Child(child));
            }
            Item::Use(item) => items.push(ModuleItem::Use(item)),
            item => {
                let key = records.len();
                records.push(make_record(path.clone(), item));
                items.push(ModuleItem::Item(key));
            }
        }
    }

    Ok(Module { name, path, items })
}

fn load_inline_module(
    name: Option<String>,
    path: Vec<String>,
    raw_items: Vec<Item>,
    records: &mut Vec<Record>,
) -> Result<Module, BoxError> {
    let mut items = Vec::new();
    for item in clean_items(raw_items) {
        match item {
            Item::Mod(mut module) => {
                let child_name = module.ident.to_string();
                let mut child_path = path.clone();
                child_path.push(child_name.clone());
                let child_items = module
                    .content
                    .take()
                    .map(|(_, items)| items)
                    .unwrap_or_default();
                items.push(ModuleItem::Child(load_inline_module(
                    Some(child_name),
                    child_path,
                    child_items,
                    records,
                )?));
            }
            Item::Use(item) => items.push(ModuleItem::Use(item)),
            item => {
                let key = records.len();
                records.push(make_record(path.clone(), item));
                items.push(ModuleItem::Item(key));
            }
        }
    }
    Ok(Module { name, path, items })
}

fn module_base_dir(file_path: &Path, is_root: bool) -> PathBuf {
    if is_root || file_path.file_name().is_some_and(|name| name == "mod.rs") {
        file_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    } else {
        file_path
            .with_extension("")
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(file_path.file_stem().unwrap())
    }
}

fn find_module_file(base_dir: &Path, name: &str) -> Result<PathBuf, BoxError> {
    let flat = base_dir.join(format!("{name}.rs"));
    if flat.exists() {
        return Ok(flat);
    }
    let nested = base_dir.join(name).join("mod.rs");
    if nested.exists() {
        return Ok(nested);
    }
    Err(format!(
        "could not find module `{name}` under {}",
        base_dir.display()
    )
    .into())
}

fn make_record(_module: Vec<String>, item: Item) -> Record {
    let is_pub = is_pub_item(&item);
    let (impl_self_locals, impl_trait_name) = impl_signature(&item);
    let names = item_names(&item);
    let mut refs = collect_idents(item.to_token_stream())
        .into_iter()
        .collect::<BTreeSet<_>>();
    let kind = record_kind(&item, &names);
    if let RecordKind::MacroUse = kind {
        if let Some(macro_name) = macro_call_name(&item) {
            refs.insert(macro_name);
        }
    }
    Record {
        item,
        names,
        refs,
        local_refs: BTreeSet::new(),
        impl_self_locals,
        impl_trait_name,
        kind,
        is_pub,
    }
}

fn record_kind(item: &Item, names: &BTreeSet<String>) -> RecordKind {
    match item {
        Item::Trait(_) | Item::TraitAlias(_) => RecordKind::Trait,
        Item::Impl(_) => RecordKind::Impl,
        Item::Macro(item) if item.ident.is_some() => RecordKind::MacroDef,
        Item::Macro(_) => RecordKind::MacroUse,
        _ if names.is_empty() => RecordKind::MacroUse,
        _ => RecordKind::Named,
    }
}

fn impl_signature(item: &Item) -> (BTreeSet<String>, Option<String>) {
    let Item::Impl(item) = item else {
        return (BTreeSet::new(), None);
    };
    let self_locals = local_self_type_idents(&item.self_ty);
    let trait_name = item
        .trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last())
        .map(|seg| seg.ident.to_string());
    (self_locals, trait_name)
}

fn item_names(item: &Item) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    match item {
        Item::Const(item) => {
            names.insert(item.ident.to_string());
        }
        Item::Enum(item) => {
            names.insert(item.ident.to_string());
        }
        Item::Fn(item) => {
            names.insert(item.sig.ident.to_string());
        }
        Item::Macro(item) => {
            if let Some(ident) = &item.ident {
                names.insert(ident.to_string());
            }
        }
        Item::Static(item) => {
            names.insert(item.ident.to_string());
        }
        Item::Struct(item) => {
            names.insert(item.ident.to_string());
        }
        Item::Trait(item) => {
            names.insert(item.ident.to_string());
        }
        Item::TraitAlias(item) => {
            names.insert(item.ident.to_string());
        }
        Item::Type(item) => {
            names.insert(item.ident.to_string());
        }
        Item::Union(item) => {
            names.insert(item.ident.to_string());
        }
        Item::Impl(_) => {}
        _ => {}
    }
    names
}

fn local_self_type_idents(ty: &Type) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_self_type_idents(ty, &mut names);
    names
}

fn collect_self_type_idents(ty: &Type, names: &mut BTreeSet<String>) {
    match ty {
        Type::Path(ty) if ty.qself.is_none() => {
            if let Some(segment) = ty.path.segments.last() {
                let name = segment.ident.to_string();
                if !is_builtin_type_name(&name) {
                    names.insert(name);
                }
            }
        }
        Type::Reference(ty) => collect_self_type_idents(&ty.elem, names),
        Type::Paren(ty) => collect_self_type_idents(&ty.elem, names),
        Type::Group(ty) => collect_self_type_idents(&ty.elem, names),
        _ => {}
    }
}

fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "Box"
            | "Rc"
            | "Arc"
            | "Vec"
            | "Option"
            | "Result"
            | "String"
            | "usize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "isize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "str"
            | "Self"
    )
}

fn is_pub_item(item: &Item) -> bool {
    match item {
        Item::Const(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Enum(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Fn(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Macro(item) => item.ident.is_some(),
        Item::Mod(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Static(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Struct(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Trait(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::TraitAlias(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Type(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Union(item) => matches!(item.vis, syn::Visibility::Public(_)),
        Item::Impl(_) => true,
        _ => false,
    }
}

fn is_pub_use(item: &ItemUse) -> bool {
    matches!(item.vis, syn::Visibility::Public(_))
}

fn collect_reexport_roots(
    tree: &UseTree,
    module_path: &[String],
    name: Option<&str>,
    library: &Library,
    out: &mut Vec<ItemKey>,
) {
    match tree {
        UseTree::Path(path) => {
            let child_path = resolve_use_path_segment(module_path, &path.ident);
            match path.tree.as_ref() {
                UseTree::Glob(_) => {
                    if let Some(index) = library.modules.get(&child_path) {
                        if let Some(module) = library.module_by_index(index) {
                            library.collect_exports(module, name, out);
                        }
                    }
                }
                other => collect_reexport_roots(other, &child_path, name, library, out),
            }
        }
        UseTree::Name(item) => {
            let item_name = item.ident.to_string();
            if name.is_none_or(|name| name == item_name) {
                for key in library.exported_matching(module_path, Some(&item_name)) {
                    out.push(key);
                }
            }
        }
        UseTree::Rename(item) => {
            let item_name = item.ident.to_string();
            let alias = item.rename.to_string();
            if name.is_none_or(|name| name == alias) {
                for key in library.exported_matching(module_path, Some(&item_name)) {
                    out.push(key);
                }
            }
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_reexport_roots(item, module_path, name, library, out);
            }
        }
        _ => {}
    }
}

fn collect_reexport_glob_roots(
    tree: &UseTree,
    module_path: &[String],
    used_names: &BTreeSet<String>,
    library: &Library,
    out: &mut Vec<ItemKey>,
) {
    match tree {
        UseTree::Path(path) => {
            let child_path = resolve_use_path_segment(module_path, &path.ident);
            match path.tree.as_ref() {
                UseTree::Glob(_) => {
                    if let Some(index) = library.modules.get(&child_path) {
                        if let Some(module) = library.module_by_index(index) {
                            library.collect_glob_exports(module, used_names, out);
                        }
                    }
                }
                other => collect_reexport_glob_roots(other, &child_path, used_names, library, out),
            }
        }
        UseTree::Name(item) => {
            let item_name = item.ident.to_string();
            if used_names.contains(&item_name) {
                for key in library.exported_matching(module_path, Some(&item_name)) {
                    out.push(key);
                }
            }
        }
        UseTree::Rename(item) if used_names.contains(&item.rename.to_string()) => {
            for key in library.exported_matching(module_path, Some(&item.ident.to_string())) {
                out.push(key);
            }
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_reexport_glob_roots(item, module_path, used_names, library, out);
            }
        }
        _ => {}
    }
}

fn resolve_use_path_segment(module_path: &[String], ident: &Ident) -> Vec<String> {
    match ident.to_string().as_str() {
        "crate" => Vec::new(),
        "self" => module_path.to_vec(),
        "super" => {
            let mut parent = module_path.to_vec();
            parent.pop();
            parent
        }
        name => {
            let mut child = module_path.to_vec();
            child.push(name.to_string());
            child
        }
    }
}

struct RewriteCrateName {
    from: String,
    to: Ident,
}

impl RewriteCrateName {
    fn new(from: &str) -> Self {
        Self {
            from: from.to_string(),
            to: crate_ident(),
        }
    }
}

impl VisitMut for RewriteCrateName {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        if let Some(first) = path.segments.first_mut() {
            if first.ident == self.from {
                first.ident = self.to.clone();
            }
        }
        visit_mut::visit_path_mut(self, path);
    }

    fn visit_use_tree_mut(&mut self, tree: &mut UseTree) {
        if let UseTree::Path(path) = tree {
            if path.ident == self.from {
                path.ident = self.to.clone();
            }
        }
        visit_mut::visit_use_tree_mut(self, tree);
    }
}

fn crate_ident() -> Ident {
    syn::parse_str::<syn::Path>("crate")
        .expect("crate path parses")
        .segments
        .first()
        .expect("crate path has a segment")
        .ident
        .clone()
}

fn collect_idents(tokens: TokenStream) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_idents_inner(tokens, &mut out);
    out
}

fn collect_idents_inner(tokens: TokenStream, out: &mut HashSet<String>) {
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => {
                out.insert(ident.to_string());
            }
            TokenTree::Group(group) => collect_idents_inner(group.stream(), out),
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
}

fn flatten_tokens(tokens: TokenStream) -> Vec<String> {
    let mut out = Vec::new();
    flatten_tokens_inner(tokens, &mut out);
    out
}

fn flatten_tokens_inner(tokens: TokenStream, out: &mut Vec<String>) {
    let mut prev_colon = false;
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => {
                if prev_colon {
                    out.push(":".to_string());
                    prev_colon = false;
                }
                out.push(ident.to_string());
            }
            TokenTree::Punct(punct) if punct.as_char() == ':' => {
                if prev_colon {
                    out.push("::".to_string());
                    prev_colon = false;
                } else {
                    prev_colon = true;
                }
            }
            TokenTree::Punct(punct) => {
                if prev_colon {
                    out.push(":".to_string());
                    prev_colon = false;
                }
                out.push(punct.as_char().to_string());
            }
            TokenTree::Literal(lit) => {
                if prev_colon {
                    out.push(":".to_string());
                    prev_colon = false;
                }
                out.push(lit.to_string());
            }
            TokenTree::Group(group) => {
                if prev_colon {
                    out.push(":".to_string());
                    prev_colon = false;
                }
                flatten_tokens_inner(group.stream(), out);
            }
        }
    }
    if prev_colon {
        out.push(":".to_string());
    }
}

fn is_ident_like(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
}

fn macro_call_name(item: &Item) -> Option<String> {
    let Item::Macro(ItemMacro { mac, .. }) = item else {
        return None;
    };
    mac.path.segments.last().map(|seg| seg.ident.to_string())
}

fn minify(tokens: TokenStream) -> String {
    let mut out = String::new();
    emit_minified(tokens, &mut out, &mut PieceKind::Other);
    out
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PieceKind {
    Atom,
    Other,
}

fn emit_minified(tokens: TokenStream, out: &mut String, prev: &mut PieceKind) {
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => push_piece(out, prev, PieceKind::Atom, &ident.to_string()),
            TokenTree::Literal(lit) => push_piece(out, prev, PieceKind::Atom, &lit.to_string()),
            TokenTree::Punct(punct) => {
                let s = punct.as_char().to_string();
                push_piece(out, prev, PieceKind::Other, &s);
            }
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                push_piece(out, prev, PieceKind::Other, open);
                emit_minified(group.stream(), out, prev);
                push_piece(out, prev, PieceKind::Other, close);
            }
        }
    }
}

fn push_piece(out: &mut String, prev: &mut PieceKind, kind: PieceKind, s: &str) {
    if !out.is_empty() && *prev == PieceKind::Atom && kind == PieceKind::Atom {
        out.push(' ');
    }
    out.push_str(s);
    *prev = kind;
}

fn check_rustc(project_root: &Path, edition: &str, output: &str) -> Result<(), BoxError> {
    let mut dir = std::env::temp_dir();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .to_string();
    dir.push(format!("oj-pack-{nonce}"));
    fs::create_dir_all(&dir)?;
    let src = dir.join("packed.rs");
    let meta = dir.join("packed.rmeta");
    fs::write(&src, output)?;

    let status = Command::new("rustc")
        .current_dir(project_root)
        .arg(format!("--edition={edition}"))
        .arg("-A")
        .arg("warnings")
        .arg("--crate-name")
        .arg("oj_pack_check")
        .arg("--emit=metadata")
        .arg(&src)
        .arg("-o")
        .arg(&meta)
        .status()?;
    let _ = fs::remove_dir_all(&dir);

    if status.success() {
        Ok(())
    } else {
        Err("rustc validation failed for packed output".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("repo root")
    }

    #[test]
    fn minifier_keeps_required_identifier_spaces() {
        let tokens: TokenStream = quote! {
            fn main() {
                let x = 1 as i64;
                let y = r#"a b"#;
                let z: &'static str = "ok";
                macro_rules! m { ($e:expr) => { $e }; }
                let _ = (x, y, z, m!(1));
            }
        };
        let packed = minify(tokens);
        assert!(packed.contains("as i64"));
        assert!(packed.contains("r#\"a b\"#"));
        assert!(packed.contains("&'static"));
        assert!(packed.contains("m!(1)"));
    }

    #[test]
    fn item_level_dce_excludes_unreachable_library_items() {
        let output = pack_project(
            repo_root(),
            "luogu_p3372",
            PackOptions {
                check: false,
                minify: false,
                max_bytes: None,
                warn_bytes: usize::MAX,
            },
        )
        .expect("pack luogu_p3372");

        assert!(output.contains("pub struct SegTree"));
        assert!(output.contains("pub struct Sum"));
        assert!(output.contains("pub struct Product"));
        assert!(!output.contains("pub mod finger_tree"));
        assert!(!output.contains("pub mod lct"));
        assert!(!output.contains("pub trait Foldable"));
        assert!(!output.contains("pub struct Identity"));
        assert!(!output.contains("pub enum Max"));
        assert!(!output.contains("pub enum Min"));
        assert!(!output.contains("pub mod test"));
        assert!(!output.contains("extern crate solution"));
    }

    #[test]
    fn formatted_output_is_readable() {
        let output = pack_project(
            repo_root(),
            "luogu_p5502",
            PackOptions {
                check: false,
                minify: false,
                max_bytes: None,
                warn_bytes: usize::MAX,
            },
        )
        .expect("pack formatted luogu_p5502");

        assert!(output.contains("\nfn main()"));
        assert!(output.contains("\nfn gcd("));
        assert!(output.lines().count() > 10);
    }

    #[test]
    fn minified_output_removes_unnecessary_newlines() {
        let output = pack_project(
            repo_root(),
            "luogu_p5502",
            PackOptions {
                check: false,
                minify: true,
                max_bytes: None,
                warn_bytes: usize::MAX,
            },
        )
        .expect("pack minified luogu_p5502");

        assert_eq!(output.lines().count(), 1);
        assert!(!output.contains("\nfn gcd("));
    }

    #[test]
    fn max_bytes_is_enforced() {
        let err = pack_project(
            repo_root(),
            "luogu_p5502",
            PackOptions {
                check: false,
                minify: true,
                max_bytes: Some(10),
                warn_bytes: usize::MAX,
            },
        )
        .expect_err("too-small max-bytes should fail");

        assert!(err.to_string().contains("exceeding --max-bytes 10"));
    }

    #[test]
    fn prunes_prelude_reexports_after_dce() {
        let output = pack_project(
            repo_root(),
            "luogu_p1383",
            PackOptions {
                check: true,
                minify: false,
                max_bytes: None,
                warn_bytes: usize::MAX,
            },
        )
        .expect("pack luogu_p1383");

        assert!(output.contains("pub mod finger_tree"));
        assert!(output.contains("FingerTreeStore"));
        assert!(!output.contains("ArcFingerTree"));
        assert!(!output.contains("ArcStore"));
    }

    #[test]
    fn packs_all_current_bins() {
        let root = repo_root();
        let package = PackageInfo::load(root).expect("metadata");
        for bin in package.bin_names() {
            let output = pack_project(
                root,
                bin,
                PackOptions {
                    check: true,
                    minify: true,
                    max_bytes: None,
                    warn_bytes: usize::MAX,
                },
            )
            .unwrap_or_else(|err| panic!("{bin} failed: {err}"));
            assert!(!output.contains("extern crate solution"));
            assert!(!output.contains("pub mod test"));
        }
    }
}
