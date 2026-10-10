// MX adapter for the bundled LibreOffice UNO engine. No network calls here.
// SDK example attribution and licenses: /office/vendor/THIRD-PARTY.md.
'use strict';
console.info('MX Office: worker adapter loaded');
Module.zetajs.then((zeta) => {
  const css = zeta.uno.com.sun.star;
  const context = zeta.getUnoComponentContext();
  let model, path, readOnly = true, generation = 0, listener, selectionListener, frameInterception;
  const property = (Name, Value) => new css.beans.PropertyValue({ Name, Value });
  const send = (message) => zeta.mainPort.postMessage(message);
  const command = url => (url.Complete || '').split('?')[0];
  const copies = ['.uno:SaveAs','.uno:SaveACopy','.uno:SaveAsRemote'];
  const saves = ['.uno:Save',...copies,'.uno:SaveAll'];
  const driveOpen = 'vnd.mx:open-drive';
  const opens = ['.uno:Open','.uno:OpenRemote','.uno:OpenFromCalc','.uno:OpenFromWriter','.uno:OpenFromDraw','.uno:OpenFromImpress','.uno:AddDirect','.uno:NewDoc','.uno:CloseDoc','.uno:CloseWin','.uno:Quit',driveOpen];
  const desktop = css.frame.Desktop.create(context);
  const openCommand = url => opens.includes(command(url)) || /^(?:private:factory\/|file:)/.test(command(url));
  const intercepted = url => saves.includes(command(url)) || openCommand(url);
  const saveDispatch = zeta.unoObject(['com.sun.star.frame.XDispatch'], {
    dispatch(url) { const name=command(url); if(!saves.includes(name)){send({cmd:'open-request'});return;}send({ cmd: copies.includes(name) ? 'save-copy-request' : 'save-request' }); },
    addStatusListener(listener, url) { listener.statusChanged(new css.frame.FeatureStateEvent({ Source: saveDispatch, FeatureURL: url, FeatureDescriptor: '', IsEnabled: saves.includes(command(url)) ? !readOnly : !['.uno:NewDoc','.uno:AddDirect','.uno:CloseDoc','.uno:CloseWin','.uno:Quit'].includes(command(url)), Requery: false, State: null })); },
    removeStatusListener() {}
  });
  function intercept(provider) {
    // Desktop-targeted File > Open (_blank) does not pass through the
    // document frame. Each provider needs its own forwarding chain.
    let slave, master;
    const interceptor = zeta.unoObject(['com.sun.star.frame.XDispatchProviderInterceptor','com.sun.star.frame.XInterceptorInfo'], {
      getInterceptedURLs() { return [...saves,...opens,'private:factory/*','file:*']; },
      queryDispatch(url, target, flags) { return intercepted(url) ? saveDispatch : slave?.queryDispatch(url,target,flags) || null; },
      queryDispatches(requests) { return requests.map(request => this.queryDispatch(request.FeatureURL,request.FrameName,request.SearchFlags)); },
      setSlaveDispatchProvider(value) { slave=value; }, getSlaveDispatchProvider() { return slave || null; },
      setMasterDispatchProvider(value) { master=value; }, getMasterDispatchProvider() { return master || null; }
    });
    provider.registerDispatchProviderInterceptor(interceptor);
    return { provider, interceptor };
  }
  // Retain both the root interceptor and its UNO forwarding references.
  const desktopInterception = intercept(desktop);
  zeta.mainPort.onmessage = ({ data }) => {
    try {
      if (data.cmd === 'open') {
        if (model) {
          if(selectionListener)model.getCurrentController().removeSelectionChangeListener(selectionListener);
          if(frameInterception)frameInterception.provider.releaseDispatchProviderInterceptor(frameInterception.interceptor);
          model.removeModifyListener(listener); model.setModified(false); model.close(true);
        }
        path = `file:///tmp/mx-office/document${data.extension}`;
        readOnly = data.readOnly || ['.ppt','.pptx','.odp'].includes(data.extension);
        generation = 0;
        model = desktop.loadComponentFromURL(path, '_default', 0, [
          property('ReadOnly', readOnly), property('MacroExecutionMode', 0),
          property('UpdateDocMode', 0), property('Hidden', false)
        ]);
        if (!model) throw new Error('This document could not be opened by the Office engine.');
        listener = zeta.unoObject(['com.sun.star.util.XModifyListener'], {
          modified() { if (model.isModified()) send({ cmd: 'modified', generation: ++generation }); },
          disposing() {}
        });
        model.addModifyListener(listener);
        const frame = model.getCurrentController().getFrame();
        frameInterception = intercept(frame);
        frame.contextChanged();
        frame.getContainerWindow().FullScreen = true;
        frame.activate();
        frame.getComponentWindow().setFocus();
        if(['.xls','.xlsx','.ods'].includes(data.extension)) {
          const controller=model.getCurrentController();
          // Initialize Calc's input handler against the loaded selection before
          // exposing keyboard readiness; do not depend on a later no-op
          // Ctrl+Home to initialize the already selected starting cell.
          controller.select(controller.getSelection());
          const announceSelection=()=>{
            try {
              const address=controller.getSelection().getRangeAddress();
              let column=address.StartColumn+1,label='';
              while(column>0){label=String.fromCharCode(65+(column-1)%26)+label;column=Math.floor((column-1)/26);}
              send({cmd:'selection',cell:`${label}${address.StartRow+1}`});
            } catch { send({cmd:'selection',cell:''}); }
          };
          selectionListener=zeta.unoObject(['com.sun.star.view.XSelectionChangeListener'],{selectionChanged:announceSelection,disposing(){}});
          controller.addSelectionChangeListener(selectionListener);announceSelection();
        } else { selectionListener=undefined;send({cmd:'selection',cell:''}); }
        send({ cmd: 'opened', id: data.id, generation });
      } else if (data.cmd === 'export') {
        if (!model || readOnly) throw new Error('This document is read-only.');
        const current = desktop.getCurrentFrame()?.getController()?.getModel();
        if (current && !zeta.sameUnoObject(model,current)) throw new Error('Open documents through MX Drive. The displayed document changed, so MX will not save it over the original file.');
        // Export a snapshot WITHOUT changing the document's modified state or
        // original URL. Only an acknowledged MX commit counts as a server save.
        // Save must include the cell currently being typed, even if the user
        // never pressed Enter before Ctrl+S or File > Save.
        if (['.xls','.xlsx','.ods'].includes(data.extension)) {
          const controller = model.getCurrentController();
          const selection = controller.getSelection();
          css.frame.DispatchHelper.create(context).executeDispatch(controller.getFrame(), '.uno:AcceptFormula', '', 0, [property('SynchronMode', true)]);
          // The pinned Calc engine does not refresh its input handler after
          // AcceptFormula. Re-select the same range through its view API, which
          // updates the input UI without changing document cells or selection.
          controller.select(selection);
        }
        const exportedGeneration = generation;
        model.storeToURL(`file:///tmp/mx-office/export${data.extension}`, [
          property('Overwrite', true), property('FilterName', data.filter)
        ]);
        send({ cmd: 'exported', id: data.id, generation: exportedGeneration });
      } else if (data.cmd === 'committed') {
        if (generation === data.generation) model.setModified(false);
      } else if (data.cmd === 'focus' && model) {
        // Component-window focus alone does not activate its document frame.
        // Activate the frame too so native dispatch/input targets this model.
        const frame = model.getCurrentController().getFrame();
        frame.activate();
        frame.getComponentWindow().setFocus();
        send({cmd:'focused',id:data.id});
      }
    } catch (error) {
      send({ cmd: 'error', id: data.id, message: String(error?.message || error) });
    }
  };
  send({ cmd: 'ready' });
}).catch((error) => { throw error; });
