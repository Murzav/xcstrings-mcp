from pathlib import Path
import sys,json,shutil,subprocess
P=Path(__file__).parent
sys.path.insert(0,str(P/'phase-b'))
import probe
R=P/'corpus-capture';R.mkdir(exist_ok=True);shutil.copytree(P/'Oracle.xcodeproj',R/'Oracle.xcodeproj',dirs_exist_ok=True)
probe.R=R
base=json.loads((P/'valid-source.xcstrings').read_text());base['strings'].pop('sub_PIPE|==|NAME')
for name in ['controls','metadata-multi','target-substitution']:
 extra=json.loads((P/'phase-b'/name/'before.xcstrings').read_text());base['strings'].update(extra['strings'])
base['strings']['machine_translated']=probe.entry(probe.su('Machine source'),probe.su('Machine cible','machine_translated'))
base['strings']['source_only_plural']=probe.entry({'variations':{'plural':{'one':probe.su('Source one %lld'),'other':probe.su('Source other %lld')}}})
probe.stage('positive-matrix',base)
# Missing locale categories from richer target language: use the same target project and explicit uk export.
project=R/'Oracle.xcodeproj/project.pbxproj';s=project.read_text().replace('\n\t\t\t\tfr,','\n\t\t\t\tfr,\n\t\t\t\tuk,');project.write_text(s)
fallback={'sourceLanguage':'en','strings':{'fallback':probe.entry({'variations':{'plural':{'one':probe.su('one source %lld'),'other':probe.su('other source %lld')}}})},'version':'1.0'}
d=R/'missing-locale-uk';d.mkdir(exist_ok=True);probe.write(d/'before.xcstrings',fallback);probe.write(R/'Localizable.xcstrings',fallback)
codes={};codes['compile']=probe.run('missing-locale-uk-compile',['xcrun','xcstringstool','compile',str(R/'Localizable.xcstrings'),'--output-directory',str(d/'compiled')])
codes['export']=probe.run('missing-locale-uk-export',['xcodebuild','-exportLocalizations','-project',str(project.parent),'-localizationPath',str(d/'export'),'-exportLanguage','uk'])
x=d/'export/uk.xcloc/Localized Contents/uk.xliff';shutil.copy2(x,d/'before.xliff')
codes['import']=probe.run('missing-locale-uk-import',['xcodebuild','-importLocalizations','-project',str(project.parent),'-localizationPath',str(x)])
shutil.copy2(R/'Localizable.xcstrings',d/'after.xcstrings');codes['reexport']=probe.run('missing-locale-uk-reexport',['xcodebuild','-exportLocalizations','-project',str(project.parent),'-localizationPath',str(d/'reexport'),'-exportLanguage','uk'])
codes['after_compile']=probe.run('missing-locale-uk-after-compile',['xcrun','xcstringstool','compile',str(R/'Localizable.xcstrings'),'--output-directory',str(d/'compiled-after')]);probe.write(d/'results.json',codes)
shutil.rmtree(d/'compiled',ignore_errors=True);shutil.rmtree(d/'compiled-after',ignore_errors=True);print('missing-locale-uk',codes,flush=True)
