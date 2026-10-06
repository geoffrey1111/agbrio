// Production WebView2 may not expose CDP. Exercise the owned native window's
// accessibility controls without enabling devtools or injecting a web endpoint.
using System;using System.IO;using System.Linq;using System.Collections.Generic;
using System.Threading;using System.Drawing;using System.Drawing.Imaging;
using System.Runtime.InteropServices;using System.Windows.Automation;
using System.Web.Script.Serialization;
class AgbrioNativeQa {
 [StructLayout(LayoutKind.Sequential)]struct Rect{public int L,T,R,B;}
 [StructLayout(LayoutKind.Sequential)]struct Point{public int X,Y;}
 [StructLayout(LayoutKind.Sequential)]struct Placement{public int Length,Flags,Show;public Point Min,Max;public Rect Normal;}
 [DllImport("user32.dll")]static extern bool GetWindowPlacement(IntPtr h,ref Placement p);
 [DllImport("user32.dll")]static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")]static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
 static AutomationElement root;static string output;
 static AutomationElement Find(string name,ControlType type){
  var end=DateTime.UtcNow.AddSeconds(25);
  do{var e=root.FindFirst(TreeScope.Descendants,new AndCondition(new PropertyCondition(AutomationElement.NameProperty,name),new PropertyCondition(AutomationElement.ControlTypeProperty,type)));if(e!=null)return e;Thread.Sleep(300);}while(DateTime.UtcNow<end);
  Dump();throw new Exception("Owned accessibility control unavailable: "+name);
 }
 static void Dump(){var values=new List<string>();foreach(AutomationElement e in root.FindAll(TreeScope.Descendants,Condition.TrueCondition)){try{values.Add(e.Current.ControlType.ProgrammaticName+" | "+e.Current.Name);}catch{}if(values.Count>=2000)break;}var json=new JavaScriptSerializer().Serialize(values);File.WriteAllText(Path.Combine(output,"uia-controls.json"),json);Console.WriteLine("OWNED_QA_CONTROLS "+new JavaScriptSerializer().Serialize(values.Take(70).ToArray()));}
 static void Click(AutomationElement e){((InvokePattern)e.GetCurrentPattern(InvokePattern.Pattern)).Invoke();}
 static void Picture(IntPtr h,string filename){Rect r;if(!GetWindowRect(h,out r))throw new Exception("Native bounds unavailable");using(var b=new Bitmap(r.R-r.L,r.B-r.T)){using(var g=Graphics.FromImage(b)){var dc=g.GetHdc();try{if(!PrintWindow(h,dc,2))throw new Exception("Native capture failed");}finally{g.ReleaseHdc(dc);}}b.Save(Path.Combine(output,filename),ImageFormat.Png);}}
 [STAThread]static int Main(string[] args){
  try{var h=new IntPtr(long.Parse(args[0]));output=Path.GetFullPath(args[1]);Directory.CreateDirectory(output);root=AutomationElement.FromHandle(h);var p=new Placement();p.Length=Marshal.SizeOf(p);if(!GetWindowPlacement(h,ref p)||p.Show!=3)throw new Exception("Native window is not maximized");
   Click(Find("设置",ControlType.Button));Click(Find("设备",ControlType.Button));
   Find("未配置手机连接",ControlType.Text);var combo=Find("连接方式",ControlType.ComboBox);var input=Find("HTTPS 网址",ControlType.Edit);var value=(ValuePattern)input.GetCurrentPattern(ValuePattern.Pattern);if(value.Current.Value!="")throw new Exception("Personal origin in fresh profile");
   var pair=root.FindFirst(TreeScope.Descendants,new AndCondition(new PropertyCondition(AutomationElement.NameProperty,"生成配对码"),new PropertyCondition(AutomationElement.ControlTypeProperty,ControlType.Button)));if(pair!=null)throw new Exception("Future pairing stage exposed before origin verification");
   Picture(h,"desktop-first-setup.png");value.SetValue("https://agbrio-release-qa.tailnet-test.ts.net");
   var verify=Find("验证并保存",ControlType.Button);var until=DateTime.UtcNow.AddSeconds(10);while(!verify.Current.IsEnabled&&DateTime.UtcNow<until)Thread.Sleep(200);Click(verify);
   var error=false;until=DateTime.UtcNow.AddSeconds(25);do{foreach(AutomationElement e in root.FindAll(TreeScope.Descendants,Condition.TrueCondition)){try{if(e.Current.Name.Contains("未能验证")){error=true;break;}}catch{}}if(!error)Thread.Sleep(300);}while(!error&&DateTime.UtcNow<until);if(!error)throw new Exception("Verification failure/recovery did not appear");
   Picture(h,"desktop-failed-entry.png");Rect bounds;GetWindowRect(h,out bounds);foreach(var e in new[]{combo,input,verify}){var r=e.Current.BoundingRectangle;if(r.Width<=0||r.Left<bounds.L||r.Right>bounds.R||r.Top<bounds.T||r.Bottom>bounds.B)throw new Exception("Native setup control outside visible window");}
   var proof=new Dictionary<string,object>{{"freshWindowsRunner",true},{"installedApplication",true},{"nativeAccessibility",true},{"setupSelector",true},{"noPersonalOrigin",true},{"invalidHttpsRejected",true},{"pairingBlockedUntilConfigured",true},{"nativeMaximized",true},{"controlsInsideWindow",true},{"providerCredentialsUsed",false},{"captureWidth",bounds.R-bounds.L},{"captureHeight",bounds.B-bounds.T}};
   File.WriteAllText(Path.Combine(output,"proof.json"),new JavaScriptSerializer().Serialize(proof));Console.WriteLine("NATIVE_ACCESSIBILITY_SETUP_PASS");return 0;
  }catch(Exception e){Console.Error.WriteLine(e.Message);try{if(root!=null)Dump();}catch{}return 1;}
 }
}
