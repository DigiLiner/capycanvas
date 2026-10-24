import argparse
import gzip
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]
XML_LANG = '{http://www.w3.org/XML/1998/namespace}lang'


def literal_message(path, key):
    source = path.read_text(encoding='utf-8')
    match = re.search(rf'^{re.escape(key)} = ([^{{}}\n]+)$(?!\n[ \t])', source, re.M)
    if not match:
        raise ValueError(f'{path}: {key} must be literal one-line text')
    return match[1]


def desktop_locale(tag):
    return {'zh-Hans': 'zh_CN', 'zh-Hant': 'zh_TW', 'pt-BR': 'pt_BR'}.get(tag, tag)


def app_names():
    registry = (ROOT / 'crates/layer-ui/src/localization_languages.rs').read_text(encoding='utf-8')
    tags = re.findall(r'\("[^"]+", "([^"]+)",', registry)
    names = {}
    for tag in tags:
        names[tag] = literal_message(ROOT / 'assets/locales' / tag / 'common.ftl', 'common-app-name')
    return names


def localized_elements(tag, values):
    elements = []
    for locale, text in values.items():
        language = desktop_locale(locale)
        locales = [None] if locale == 'en' else list(dict.fromkeys([language, language.replace('_', '-')]))
        for language in locales:
            element = ET.Element(tag, {} if language is None else {XML_LANG: language})
            element.text = text
            elements.append(element)
    return elements


def replace_elements(parent, tag, elements):
    existing = parent.findall(tag)
    index = list(parent).index(existing[0]) if existing else len(parent)
    for element in existing:
        parent.remove(element)
    for offset, element in enumerate(elements):
        parent.insert(index + offset, element)


def write_linux(output, names):
    keys = ['summary', 'description', 'caption-paint', 'caption-sketch', 'caption-photo',
            'keyword-drawing', 'keyword-painting', 'keyword-illustration']
    copy = {key: {locale: literal_message(ROOT / 'assets/locales' / locale / 'store.ftl', 'store-' + key)
                  for locale in names} for key in keys}
    desktop = output / 'art.capycanvas.CapyCanvas.desktop'
    source = desktop.read_text(encoding='utf-8')
    keywords = {locale: ';'.join(copy['keyword-' + key][locale] for key in ['drawing', 'painting', 'illustration']) + ';'
                for locale in names}
    for field, values in [('Name', names), ('Comment', copy['summary']), ('Keywords', keywords)]:
        source = re.sub(rf'^{field}\[[^]]+\]=.*\n', '', source, flags=re.M)
        labels = ''.join(f'{field}{"" if locale == "en" else "[" + desktop_locale(locale) + "]"}={value}\n'
                         for locale, value in values.items())
        source = re.sub(rf'^{field}=.*\n', lambda _: labels, source, count=1, flags=re.M)
    desktop.write_text(source, encoding='utf-8')
    path = output / 'art.capycanvas.CapyCanvas.metainfo.xml'
    component = ET.parse(path).getroot()
    replace_elements(component, 'name', localized_elements('name', names))
    replace_elements(component, 'summary', localized_elements('summary', copy['summary']))
    replace_elements(component.find('description'), 'p', localized_elements('p', copy['description']))
    keyword_group = ET.Element('keywords')
    for key in ['drawing', 'painting', 'illustration']:
        keyword_group.extend(localized_elements('keyword', copy['keyword-' + key]))
    replace_elements(component, 'keywords', [keyword_group])
    for screenshot in component.findall('screenshots/screenshot'):
        scene = Path(screenshot.findtext('image')).name.split('-', 1)[0]
        replace_elements(screenshot, 'caption', localized_elements('caption', copy['caption-' + scene]))
    ET.indent(component, space='  ')
    path.write_text('<?xml version="1.0" encoding="UTF-8"?>\n' + ET.tostring(component, encoding='unicode') + '\n', encoding='utf-8')


def write_catalog_languages(path):
    catalog = ET.fromstring(gzip.decompress(path.read_bytes()))
    component = catalog.find("component[id='art.capycanvas.CapyCanvas']")
    if component is None:
        raise ValueError('Catalog must contain art.capycanvas.CapyCanvas')
    languages = ET.Element('languages')
    for locale in app_names():
        ET.SubElement(languages, 'lang', {'percentage': '100'}).text = desktop_locale(locale)
    replace_elements(component, 'languages', [languages])
    ET.indent(catalog, space='  ')
    path.write_bytes(gzip.compress(ET.tostring(catalog, encoding='utf-8', xml_declaration=True), mtime=0))


def write_names(platform, output):
    names = app_names()
    if platform == 'linux':
        write_linux(output, names)
        return
    for tag, name in names.items():
        if platform == 'android':
            directory = output / ('values' if tag == 'en' else 'values-b+' + tag.replace('-', '+'))
            content = f'<?xml version="1.0" encoding="utf-8"?>\n<resources><string name="capy_app_name">{escape(name)}</string></resources>\n'
            filename = 'app_name.xml'
        else:
            directory = output / f'{tag}.lproj'
            content = ''.join(f'{key} = {json.dumps(name, ensure_ascii=False)};\n' for key in ['CFBundleName', 'CFBundleDisplayName'])
            filename = 'InfoPlist.strings'
        directory.mkdir(parents=True, exist_ok=True)
        (directory / filename).write_text(content, encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('platform', choices=['json', 'android', 'apple', 'linux', 'catalog'])
    parser.add_argument('output', nargs='?', type=Path)
    args = parser.parse_args()
    if args.platform == 'json':
        print(json.dumps(app_names()))
    elif args.output is None:
        parser.error('output is required')
    elif args.platform == 'catalog':
        write_catalog_languages(args.output)
    else:
        write_names(args.platform, args.output)
