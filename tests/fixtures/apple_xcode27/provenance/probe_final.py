from probe import *
# Catalog missing targets: see which unfinished XLIFF states import at all.
a=json.loads((R/'states-import/before.xcstrings').read_text())
def missing_targets(x):
 shutil.copy2(R/'states-import/export/fr.xcloc/Localized Contents/fr.xliff',x)
 data=json.loads((R/'Localizable.xcstrings').read_text())
 for e in data['strings'].values():e['localizations'].pop('fr',None)
 write(R/'Localizable.xcstrings',data)
stage('states-new-target',a,missing_targets)
# State qualifier data from actual Xcode binary, including machine translated state.
qualifiers=['exact-match','fuzzy-match','leveraged-mt','x-apple-machine-translated','x-machine-translated']
b={'sourceLanguage':'en','strings':{q:entry(su('source '+q),su('old '+q)) for q in qualifiers},'version':'1.0'}
b['strings']['machine_translated']=entry(su('Machine'),su('Machine fr','machine_translated'))
def quals(x):
 t=E.parse(x)
 for u in t.findall('.//x:trans-unit',NS):
  k=u.attrib['id'];v=u.find('x:target',NS)
  if k in qualifiers:v.set('state','translated');v.set('state-qualifier',k);v.text='changed '+k
 t.write(x,encoding='utf-8',xml_declaration=True)
stage('qualifiers',b,quals)
# Multiple catalogs with same key; concrete original mapping.
p=R/'Oracle.xcodeproj/project.pbxproj';s=p.read_text()
s=s.replace('/* End PBXBuildFile section */','A0000000000000000000000D = {isa = PBXBuildFile; fileRef = A0000000000000000000000E; };\n/* End PBXBuildFile section */')
s=s.replace('/* End PBXFileReference section */','A0000000000000000000000E = {isa = PBXFileReference; lastKnownFileType = text.json.xcstrings; path = Custom.xcstrings; sourceTree = "<group>"; };\n/* End PBXFileReference section */')
s=s.replace('A00000000000000000000005 /* Localizable.xcstrings */,','A00000000000000000000005 /* Localizable.xcstrings */,\nA0000000000000000000000E,')
s=s.replace('A0000000000000000000000A /* Localizable.xcstrings in Resources */,','A0000000000000000000000A /* Localizable.xcstrings in Resources */,\nA0000000000000000000000D,')
p.write_text(s)
c={'sourceLanguage':'en','strings':{'shared':entry(su('CUSTOM source'),su('CUSTOM cible'))},'version':'1.0'};write(R/'Custom.xcstrings',c)
a={'sourceLanguage':'en','strings':{'shared':entry(su('LOCAL source'),su('LOCAL cible'))},'version':'1.0'}
stage('multiple-catalogs',a)
write(R/'multiple-catalogs/custom-after.xcstrings',json.loads((R/'Custom.xcstrings').read_text()))
